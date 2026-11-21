//! A colour fade's frame patches the display list in place and restyles
//! nothing; tweens that do need the cascade share ONE refresh per frame.
//!
//! The flat Button fades its face on hover (ANIM8's `decl::state_fade`):
//! `background` and the four `border-*-color`s, 120 ms. Only a text colour and
//! a solid background were patchable, so the four border sides took the slow
//! path every frame: each called `restyle_user_property` - a whole-DOM
//! compact-cache and inheritance rebuild (2.75 ms in AzWidgets, ANIMFRAME8) -
//! and one unpatchable tween forces the whole frame onto a display-list
//! rebuild. A hovered button cost four whole-DOM restyles and a full display
//! list per frame for a colour change on one box.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    events::ProcessEventResult,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::ColorU,
    layout::LayoutWidth,
    property::CssProperty,
    style::{
        StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomColor,
        StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopColor,
    },
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange,
    solver3::display_list::DisplayListItem, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const FACE: ColorU = ColorU {
    r: 0x33,
    g: 0x66,
    b: 0xff,
    a: 0xff,
};
const EDGE: ColorU = ColorU {
    r: 0x22,
    g: 0x44,
    b: 0xaa,
    a: 0xff,
};
const REST_EDGE: ColorU = ColorU {
    r: 0xaa,
    g: 0xaa,
    b: 0xaa,
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

/// The top border colour the current display list paints for `node`.
fn painted_top_edge(lw: &LayoutWindow, node: NodeId) -> Option<ColorU> {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    dl.items.iter().enumerate().find_map(|(i, item)| {
        if dl.node_mapping.get(i).copied().flatten() != Some(node) {
            return None;
        }
        match item {
            DisplayListItem::Border { colors, .. } => colors
                .top
                .as_ref()
                .and_then(|c| c.get_property())
                .map(|c| c.inner),
            _ => None,
        }
    })
}

fn face_window() -> (LayoutWindow, NodeId) {
    let lw = window_with(Dom::create_body().with_child(
        Dom::create_div().with_class("face".into()).with_css(
            "width: 80px; height: 24px; background: #eeeeee; border: 1px solid #aaaaaa; \
                 animation: background 120ms linear, border-top-color 120ms linear, \
                 border-right-color 120ms linear, border-bottom-color 120ms linear, \
                 border-left-color 120ms linear;",
        ),
    ));
    let face = node_with_class(&lw, "face");
    (lw, face)
}

#[test]
fn a_face_fade_frame_is_patched_in_place_and_restyles_nothing() {
    let (mut lw, face) = face_window();
    assert_eq!(
        painted_top_edge(&lw, face),
        Some(REST_EDGE),
        "harness: the face paints its resting border"
    );
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: face,
        props: vec![
            CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
                StyleBackgroundContent::Color(FACE),
            ])),
            CssProperty::const_border_top_color(StyleBorderTopColor { inner: EDGE }),
            CssProperty::const_border_right_color(StyleBorderRightColor { inner: EDGE }),
            CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: EDGE }),
            CssProperty::const_border_left_color(StyleBorderLeftColor { inner: EDGE }),
        ],
        override_only: false,
    });
    assert_eq!(
        lw.css_transitions.len(),
        5,
        "harness: the face declares a tween for each of the five properties"
    );

    let mut edges = Vec::new();
    for frame in 0..3 {
        let before = epoch(&lw);
        let _ = lw.tick_animations(0.016);
        let work = lw.take_animation_frame_work();
        assert_eq!(
            work,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "frame {frame}: every value of the fade was patched into the display list in place \
             - the frame owes a repaint, not a rebuild (pending css dirt: {:?})",
            lw.pending_css_dirty
        );
        assert_eq!(
            epoch(&lw),
            before,
            "frame {frame}: a patched colour frame must not re-run the cascade"
        );
        edges.push(painted_top_edge(&lw, face));
    }
    assert!(
        edges
            .iter()
            .any(|e| e.is_some_and(|c| c != REST_EDGE && c != EDGE)),
        "the painted border must pass between its two colours, frames: {edges:?}"
    );

    for _ in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = lw.tick_animations(0.016);
        let _ = lw.take_animation_frame_work();
    }
    // Any later list built for another reason (a caret blink, a relayout)
    // must paint the settled colour: the patched frames kept the style the
    // builder reads in step with the list they patched.
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(
        painted_top_edge(&lw, face),
        Some(EDGE),
        "a list rebuilt after the fade must paint the faded-to border"
    );
}

/// Two `width` tweens: a size has no in-place patch and no GPU value, so
/// each tween writes its override and the frame refreshes the cascade once
/// for both. (This used two `opacity` tweens until FIX9-PAINT 2.8 put a CSS
/// opacity fade on the GPU value path: an opacity frame whose layer exists
/// restyles nothing at all, which the window's
/// `a_css_opacity_tween_frame_after_the_first_is_values_only` pins.)
#[test]
fn tweens_that_restyle_share_one_cascade_refresh_per_frame() {
    let mut lw = window_with(
        Dom::create_body()
            .with_child(
                Dom::create_div()
                    .with_class("a".into())
                    .with_css("width: 20px; height: 20px; animation: width 150ms linear;"),
            )
            .with_child(
                Dom::create_div()
                    .with_class("b".into())
                    .with_css("width: 20px; height: 20px; animation: width 150ms linear;"),
            ),
    );
    for class in ["a", "b"] {
        let node = node_with_class(&lw, class);
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: node,
            props: vec![CssProperty::width(LayoutWidth::px(40.0))],
            override_only: false,
        });
    }
    assert_eq!(lw.css_transitions.len(), 2, "harness: two width tweens");
    let before = epoch(&lw);
    let _ = lw.tick_animations(0.016);
    let refreshes = epoch(&lw).wrapping_sub(before);
    assert_eq!(
        refreshes, 1,
        "two tweens that need the cascade in one frame must share ONE refresh of it (each \
         refresh rebuilds the compact cache of the whole DOM), the frame ran {refreshes} epoch \
         bumps"
    );
}
