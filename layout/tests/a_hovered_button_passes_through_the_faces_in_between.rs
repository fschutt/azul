//! A hovered button PASSES THROUGH the faces between its resting face and its
//! hover face - every part of it: the gradient layers, the glow, the edge (the
//! user, 2026-10-07: "the hover animation of the default blue button doesn't
//! work it switches immediately to the hover state without interpolating the
//! gradient, glow, etc.").
//!
//! ANIM8 made a pointer state change START the transitions a button declares
//! (`a_button_fades_into_its_hover_face`). Started is not shown: every frame
//! paints `from.interpolate(to, t)`, and that knew colours and linear
//! gradients whose stops stay in place. flora's blue stone - the primary
//! button - is six layers, three of them radial, and its streak moves its
//! stops under the pointer; its glow is two box shadows. None of it had a
//! value in between, so the stone held its face and jumped. flat's neutral
//! button on a dark desktop rests on one colour and hovers to a gradient: the
//! same jump.
//!
//! The check: lay the button out, hover it the way the shell does, run the
//! fade's frames the way the frame driver does (the list is rebuilt when the
//! tick says the frame owes it), and read what the display list paints half
//! way through - and that every face property the hover changes fades at the
//! pace flora.css gives it.

use std::sync::Arc;

use azul_core::{
    app_theme::ThemeScope,
    dom::{Dom, DomId, NodeId},
    events::ProcessEventResult,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{ActiveChange, HoverChange, StyledNodeState},
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_css::{
    props::{
        basic::{
            animation::AnimationTiming,
            color::{ColorOrSystem, ColorU},
        },
        property::CssPropertyType,
        style::box_shadow::{BoxShadowClipMode, StyleBoxShadow},
    },
    system::defaults,
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    widgets::{
        button::{Button, ButtonType},
        themes::{flora, UiTheme},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// How a button gets its look: pinned to a theme (`with_theme`), or
/// following the app theme - the way every app builds its buttons.
#[derive(Debug, Clone, Copy)]
enum Look {
    Pinned(UiTheme),
    Following(UiTheme),
}

/// `button` alone in a 640x480 window, light or `dark`, built and styled for
/// the app theme the way a window's layout pass does it.
fn window(button: &Button, look: Look, dark: bool) -> LayoutWindow {
    let app_theme = match look {
        Look::Pinned(_) => UiTheme::Flat,
        Look::Following(theme) => theme,
    };
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(if dark {
        defaults::macos_modern_dark()
    } else {
        defaults::macos_modern_light()
    }));
    lw.mode = OptionDarkLightMode::None;
    lw.app_theme = AzString::from(app_theme.name());
    let mut ws = FullWindowState::default();
    ws.mode = if dark {
        DarkLightMode::Dark
    } else {
        DarkLightMode::Light
    };
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    let dom = {
        let _scope = ThemeScope::enter(AzString::from(app_theme.name()));
        let button = match look {
            Look::Pinned(theme) => button.clone().with_theme(theme),
            Look::Following(_) => button.clone(),
        };
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(button.dom())
    };
    let styled = lw.style_user_dom(dom);
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

fn mode_pinned_by_env() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
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

/// The pointer enters `node`, as the shell applies it (`apply_hover_restyle`):
/// the restyle, the transitions it starts, and the list rebuilt for the
/// paint-only change.
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
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
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
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
}

/// One frame of the animation driver: the clock steps by `dt`, then the
/// frame does the work the tick says it owes, as the shells do it.
fn frame(lw: &mut LayoutWindow, dt: f32) {
    let _ = lw.tick_animations(dt);
    match lw.take_animation_frame_work() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
            lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
        }
        ProcessEventResult::ShouldReRenderCurrentWindow | ProcessEventResult::DoNothing => {}
        other => panic!("a face fade moves no box, yet its frame owes {other:?}"),
    }
}

/// The frames of the next `seconds`, 60 a second.
fn run(lw: &mut LayoutWindow, seconds: f32) {
    const FRAME: f32 = 1.0 / 60.0;
    let mut left = seconds;
    while left > 1e-4 {
        let dt = left.min(FRAME);
        frame(lw, dt);
        left -= dt;
    }
}

/// Every transition in flight on `node`: (property, duration in ms, timing).
fn started(lw: &LayoutWindow, node: NodeId) -> Vec<(&'static str, u32, AnimationTiming)> {
    lw.css_transitions
        .iter()
        .filter(|t| t.node == node)
        .map(|t| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let ms = (t.duration_s * 1000.0).round() as u32;
            (t.prop_type.to_str(), ms, t.timing)
        })
        .collect()
}

/// How long the transition of `node`'s `ty` runs, in seconds.
fn duration_of(lw: &LayoutWindow, node: NodeId, ty: CssPropertyType) -> f32 {
    lw.css_transitions
        .iter()
        .find(|t| t.node == node && t.prop_type == ty)
        .map(|t| t.duration_s)
        .unwrap_or_else(|| {
            panic!(
                "harness: the hover starts a {} transition (started: {:?})",
                ty.to_str(),
                started(lw, node)
            )
        })
}

/// What the display list paints for `node`, in paint order.
fn painted(lw: &LayoutWindow, node: NodeId) -> Vec<DisplayListItem> {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    dl.items
        .iter()
        .enumerate()
        .filter(|(i, _)| dl.node_mapping.get(*i).copied().flatten() == Some(node))
        .map(|(_, item)| item.clone())
        .collect()
}

/// The background layers `node` paints, bottom first.
fn face(lw: &LayoutWindow, node: NodeId) -> Vec<DisplayListItem> {
    painted(lw, node)
        .into_iter()
        .filter(|item| {
            matches!(
                item,
                DisplayListItem::Rect { .. }
                    | DisplayListItem::LinearGradient { .. }
                    | DisplayListItem::RadialGradient { .. }
                    | DisplayListItem::ConicGradient { .. }
            )
        })
        .collect()
}

/// The shadows `node` casts outside its box, in slot order.
fn glow(lw: &LayoutWindow, node: NodeId) -> Vec<StyleBoxShadow> {
    painted(lw, node)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::BoxShadow { shadow, .. }
                if shadow.clip_mode == BoxShadowClipMode::Outset =>
            {
                Some(shadow)
            }
            _ => None,
        })
        .collect()
}

fn concrete(c: ColorOrSystem) -> ColorU {
    match c {
        ColorOrSystem::Color(c) => c,
        ColorOrSystem::System(s) => panic!("an unresolved system colour reached the list: {s:?}"),
    }
}

/// `(offset in %, colour)` of every stop of a painted linear gradient.
fn linear_stops(item: &DisplayListItem) -> Vec<(f32, ColorU)> {
    let DisplayListItem::LinearGradient { gradient, .. } = item else {
        panic!("not a linear gradient: {item:?}");
    };
    gradient
        .stops
        .as_ref()
        .iter()
        .map(|s| (s.offset.normalized() * 100.0, concrete(s.color)))
        .collect()
}

/// `x` lies strictly between `a` and `b` - or is both, when they are one.
fn between(a: f32, x: f32, b: f32) -> bool {
    if (a - b).abs() < 1e-3 {
        (x - a).abs() < 1e-3
    } else {
        x > a.min(b) && x < a.max(b)
    }
}

/// Every channel of `x` within `a`..`b`, and `x` neither of them.
fn colour_between(a: ColorU, x: ColorU, b: ColorU) -> bool {
    let ch = |c: ColorU| [c.r, c.g, c.b, c.a];
    let within = ch(a)
        .iter()
        .zip(ch(x))
        .zip(ch(b))
        .all(|((a, x), b)| x >= *a.min(&b) && x <= *a.max(&b));
    within && x != a && x != b
}

const LIGHT_AND_FOLLOWING_FLORA: [Look; 2] = [
    Look::Pinned(UiTheme::Flora),
    Look::Following(UiTheme::Flora),
];

#[test]
fn the_blue_stone_passes_through_the_faces_between_rest_and_hover() {
    for look in LIGHT_AND_FOLLOWING_FLORA {
        let primary = Button::with_type(AzString::from("Send"), ButtonType::Primary);
        let mut lw = window(&primary, look, false);
        let node = button_node(&lw);
        let rest = face(&lw, node);
        assert_eq!(
            rest.len(),
            6,
            "{look:?}: harness - the stone is six layers: {rest:?}"
        );

        hover(&mut lw, node);
        let half = duration_of(&lw, node, CssPropertyType::BackgroundContent) / 2.0;
        run(&mut lw, half);
        let mid = face(&lw, node);
        run(&mut lw, half + 1.0);
        assert!(
            lw.css_transitions.iter().all(|t| t.node != node),
            "{look:?}: harness - the fade is over: {:?}",
            started(&lw, node)
        );
        let lit = face(&lw, node);
        assert_eq!(
            lit.len(),
            6,
            "{look:?}: harness - the lit stone is six layers: {lit:?}"
        );

        assert_eq!(
            mid.len(),
            6,
            "{look:?}: half way, the stone is its six layers, each on its way: {mid:?}"
        );
        // The gem and the rig are the same lit and at rest, and stay so.
        for i in 0..5 {
            let same = match (&rest[i], &mid[i]) {
                (
                    DisplayListItem::RadialGradient { gradient: a, .. },
                    DisplayListItem::RadialGradient { gradient: b, .. },
                ) => a == b,
                (
                    DisplayListItem::LinearGradient { gradient: a, .. },
                    DisplayListItem::LinearGradient { gradient: b, .. },
                ) => a == b,
                _ => false,
            };
            assert!(
                same,
                "{look:?}: layer {i} does not change under the pointer and must stay as it is: \
                 {:?} -> {:?}",
                rest[i], mid[i]
            );
        }
        // The specular streak brightens and widens: half way it is between
        // the two, stop by stop - not the lit streak already.
        let (r, m, l) = (
            linear_stops(&rest[5]),
            linear_stops(&mid[5]),
            linear_stops(&lit[5]),
        );
        assert_eq!(
            m.len(),
            r.len(),
            "{look:?}: the streak keeps its stops: {m:?}"
        );
        for (i, ((rs, ms), ls)) in r.iter().zip(&m).zip(&l).enumerate() {
            let strength = |c: ColorU| f32::from(c.a);
            assert!(
                between(rs.0, ms.0, ls.0)
                    && between(strength(rs.1), strength(ms.1), strength(ls.1)),
                "{look:?}: half way, stop {i} of the streak lies between its resting and its lit \
                 place and strength: rest {rs:?}, painted {ms:?}, lit {ls:?}"
            );
        }
    }
}

#[test]
fn the_blue_stones_glow_fades_in() {
    for look in LIGHT_AND_FOLLOWING_FLORA {
        let primary = Button::with_type(AzString::from("Send"), ButtonType::Primary);
        let mut lw = window(&primary, look, false);
        let node = button_node(&lw);
        let rest = glow(&lw, node);
        assert_eq!(
            rest.len(),
            1,
            "{look:?}: harness - at rest the stone casts one shadow: {rest:?}"
        );

        hover(&mut lw, node);
        let half = duration_of(&lw, node, CssPropertyType::BoxShadowLeft) / 2.0;
        run(&mut lw, half);
        let mid = glow(&lw, node);
        run(&mut lw, half + 1.0);
        let lit = glow(&lw, node);
        assert_eq!(
            lit.len(),
            2,
            "{look:?}: harness - lit, the gold rim and the bloom: {lit:?}"
        );

        assert_eq!(
            mid.len(),
            2,
            "{look:?}: half way, the rim is coming and the cast shadow is becoming the bloom: \
             {mid:?}"
        );
        let (rim, lit_rim) = (mid[0], lit[0]);
        assert!(
            rim.color.a > 0 && rim.color.a < lit_rim.color.a,
            "{look:?}: half way, the gold rim is part of its strength, not all of it: {rim:?} \
             (lit: {lit_rim:?})"
        );
        assert!(
            between(
                0.0,
                rim.spread_radius.inner.number.get(),
                lit_rim.spread_radius.inner.number.get()
            ),
            "{look:?}: and part of its width: {rim:?} (lit: {lit_rim:?})"
        );
        let (cast, bloom, lit_bloom) = (rest[0], mid[1], lit[1]);
        assert!(
            between(
                cast.blur_radius.inner.number.get(),
                bloom.blur_radius.inner.number.get(),
                lit_bloom.blur_radius.inner.number.get()
            ),
            "{look:?}: half way, the shadow under the stone spreads toward the bloom: {cast:?} -> \
             {bloom:?} -> {lit_bloom:?}"
        );
        assert!(
            colour_between(cast.color, bloom.color, lit_bloom.color),
            "{look:?}: and turns toward its gold: {cast:?} -> {bloom:?} -> {lit_bloom:?}"
        );
    }
}

#[test]
fn a_flat_button_on_a_dark_desktop_fades_from_its_desktop_face_into_its_hover_gradient() {
    if mode_pinned_by_env() {
        return;
    }
    for look in [Look::Pinned(UiTheme::Flat), Look::Following(UiTheme::Flat)] {
        let mut lw = window(&Button::create(AzString::from("Cancel")), look, true);
        let node = button_node(&lw);
        let rest = face(&lw, node);
        let [DisplayListItem::Rect { color: desk, .. }] = rest.as_slice() else {
            panic!(
                "{look:?}: harness - on a dark desktop the neutral button rests on the desktop's \
                 button face, one colour: {rest:?}"
            );
        };
        let desk = *desk;

        hover(&mut lw, node);
        let half = duration_of(&lw, node, CssPropertyType::BackgroundContent) / 2.0;
        run(&mut lw, half);
        let mid = face(&lw, node);
        run(&mut lw, half + 1.0);
        let lit = face(&lw, node);
        let [lit_face @ DisplayListItem::LinearGradient { .. }] = lit.as_slice() else {
            panic!("{look:?}: harness - hovered, Office's gradient: {lit:?}");
        };

        let [mid_face] = mid.as_slice() else {
            panic!("{look:?}: half way, the face is one layer on its way: {mid:?}");
        };
        let (m, l) = (linear_stops(mid_face), linear_stops(lit_face));
        assert_eq!(m.len(), l.len(), "{look:?}: {m:?} / {l:?}");
        for (i, ((_, mc), (_, lc))) in m.iter().zip(&l).enumerate() {
            assert!(
                colour_between(desk, *mc, *lc),
                "{look:?}: half way, stop {i} of the face is between the desktop's face {desk:?} \
                 and the hover gradient's {lc:?}, not either: painted {mc:?}"
            );
        }
    }
}

/// The face properties a pointer state can change.
const FACE: [CssPropertyType; 11] = [
    CssPropertyType::BackgroundContent,
    CssPropertyType::BorderTopColor,
    CssPropertyType::BorderRightColor,
    CssPropertyType::BorderBottomColor,
    CssPropertyType::BorderLeftColor,
    CssPropertyType::TextColor,
    CssPropertyType::BoxShadowLeft,
    CssPropertyType::BoxShadowRight,
    CssPropertyType::BoxShadowTop,
    CssPropertyType::BoxShadowBottom,
    CssPropertyType::TextShadow,
];

/// The face properties `node` changes under the pointer: what it resolves to
/// in its state now against what it resolves to hovered.
fn hover_changes(lw: &LayoutWindow, node: NodeId) -> Vec<CssPropertyType> {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let cache = styled.get_css_property_cache();
    let nd = &styled.node_data.as_container()[node];
    let now: StyledNodeState = styled.styled_nodes.as_container()[node].styled_node_state;
    let hovered = StyledNodeState { hover: true, ..now };
    FACE.iter()
        .copied()
        .filter(|ty| {
            cache.get_property(nd, &node, &now, ty) != cache.get_property(nd, &node, &hovered, ty)
        })
        .collect()
}

/// flora.css's pace for `ty` on a button of `kind`: a stone and the
/// metal-edged command (`.btn-primary`, `.btn-hero-primary`) move their
/// light over `--fl-dur-slow` on `--fl-ease` and their edge and shadows over
/// `--fl-dur-slow`, `ease`; paper and the quiet note (`.btn`) change state
/// over `--fl-dur` on `--fl-ease`.
fn flora_pace(kind: ButtonType, ty: CssPropertyType) -> (u32, AnimationTiming) {
    let light_moves = matches!(
        kind,
        ButtonType::Primary
            | ButtonType::Success
            | ButtonType::Danger
            | ButtonType::Warning
            | ButtonType::Info
            | ButtonType::Illuminated
    );
    match (light_moves, ty) {
        (true, CssPropertyType::BackgroundContent) => (flora::FL_DUR_SLOW_MS, flora::FL_EASE),
        (true, _) => (flora::FL_DUR_SLOW_MS, AnimationTiming::Ease),
        (false, _) => (flora::FL_DUR_MS, flora::FL_EASE),
    }
}

#[test]
fn every_flora_button_fades_everything_its_hover_changes_at_flora_css_pace() {
    for dark in [false, true] {
        if dark && mode_pinned_by_env() {
            continue;
        }
        for kind in [
            ButtonType::Default,
            ButtonType::Primary,
            ButtonType::Danger,
            ButtonType::Illuminated,
            ButtonType::Link,
        ] {
            for look in LIGHT_AND_FOLLOWING_FLORA {
                let mut lw = window(&Button::with_type(AzString::from("Go"), kind), look, dark);
                let node = button_node(&lw);
                let changed = hover_changes(&lw, node);
                assert!(
                    !changed.is_empty(),
                    "{kind:?} {look:?} dark={dark}: harness - a flora command changes its face \
                     under the pointer"
                );
                hover(&mut lw, node);
                for ty in &changed {
                    let Some(tr) = lw
                        .css_transitions
                        .iter()
                        .find(|t| t.node == node && t.prop_type == *ty)
                    else {
                        panic!(
                            "{kind:?} {look:?} dark={dark}: the hover changes {} and nothing fades \
                             it - it snaps (started: {:?})",
                            ty.to_str(),
                            started(&lw, node)
                        );
                    };
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ms = (tr.duration_s * 1000.0).round() as u32;
                    assert_eq!(
                        (ms, tr.timing),
                        flora_pace(kind, *ty),
                        "{kind:?} {look:?} dark={dark}: {} fades at flora.css's pace",
                        ty.to_str()
                    );
                }

                // A press is the one fast movement: the same curves, over
                // `--fl-dur-fast` (`.btn:active { transition-duration: .. }`).
                run(&mut lw, 2.0);
                press(&mut lw, node);
                for tr in lw.css_transitions.iter().filter(|t| t.node == node) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ms = (tr.duration_s * 1000.0).round() as u32;
                    assert_eq!(
                        (ms, tr.timing),
                        (flora::FL_DUR_FAST_MS, flora_pace(kind, tr.prop_type).1),
                        "{kind:?} {look:?} dark={dark}: pressed, {} moves fast on its own curve",
                        tr.prop_type.to_str()
                    );
                }
            }
        }
    }
}
