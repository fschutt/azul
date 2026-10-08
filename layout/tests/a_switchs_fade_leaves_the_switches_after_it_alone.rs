//! A Switch's fade leaves the switches after it alone.
//!
//! The track's colour fade is patched into the display list in place
//! (`DisplayList::patch_paint_colors`), over the items of the track's subtree
//! - a text colour reaches its descendants that way - whose colour is the
//! fade's `from`. The subtree's end came from `subtree_len`, which measured a
//! node WITHOUT a next sibling to the end of the whole tree. A switch is the
//! last child of its settings row, so its range ran over every node after
//! it: toggling "Group thousands" off on AzCalculator's settings page faded
//! "Keep the history"'s track - also on, the same green - to grey with it,
//! frame by frame (SWITCH13: the second track was damaged on every frame of
//! the first one's glide).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::ColorU,
    property::{CssProperty, CssPropertyType},
    style::StyleBackgroundContent,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::ContentChange,
    solver3::display_list::DisplayListItem,
    widgets::{
        switch::{build_knob_style, build_track_style, Switch},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A settings row: a label, then the switch (flat's: a solid track) - the
/// row's last child.
fn row(label: &str, on: bool) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(Dom::create_span_with_text(label))
        .with_child(Switch::create(on).with_theme(UiTheme::Flat).dom())
}

fn tracks(lw: &LayoutWindow) -> Vec<NodeId> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .filter(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == "__azul-native-switch"))
        })
        .collect()
}

/// The solid colour the current display list fills `node`'s box with.
fn painted_fill(lw: &LayoutWindow, node: NodeId) -> Option<ColorU> {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    dl.items.iter().enumerate().find_map(|(i, item)| {
        if dl.node_mapping.get(i).copied().flatten() != Some(node) {
            return None;
        }
        match item {
            DisplayListItem::Rect { color, .. } => Some(*color),
            _ => None,
        }
    })
}

/// The value the Switch's style gives property `ty` - what its click handler
/// writes.
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

/// The flat track's face, on or off: what the click handler writes.
fn face(on: bool) -> CssProperty {
    styled(&build_track_style(on), CssPropertyType::BackgroundContent)
}

fn solid(prop: &CssProperty) -> ColorU {
    match prop {
        CssProperty::BackgroundContent(v) => match v.get_property().map(|l| l.as_ref()) {
            Some([StyleBackgroundContent::Color(c)]) => *c,
            other => panic!("the track's face is one colour, got {other:?}"),
        },
        other => panic!("not a background: {other:?}"),
    }
}

#[test]
fn a_switchs_fade_leaves_the_switches_after_it_alone() {
    let mut dom = Dom::create_body()
        .with_child(row("Group thousands", true))
        .with_child(row("Keep the history", true));
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");

    let [first, second] = tracks(&lw)[..] else {
        panic!("harness: two switches");
    };
    let on = solid(&face(true));
    assert_eq!(
        painted_fill(&lw, second),
        Some(on),
        "harness: the second switch is on"
    );

    // The first switch is clicked off: its handler's two writes.
    let knob = NodeId::new(first.index() + 1);
    for (node, prop) in [
        (first, face(false)),
        (
            knob,
            styled(&build_knob_style(false), CssPropertyType::Transform),
        ),
    ] {
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: node,
            props: vec![prop],
            override_only: false,
        });
    }
    let mut seconds = Vec::new();
    for _ in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = lw.tick_animations(1.0 / 60.0);
        let _ = lw.take_animation_frame_work();
        seconds.push(painted_fill(&lw, second));
    }
    assert!(
        seconds.iter().all(|fill| *fill == Some(on)),
        "the second switch was never touched - it must stay on through the first one's fade, \
         painted per frame: {seconds:?}"
    );
    let off = solid(&face(false));
    assert_eq!(
        painted_fill(&lw, first),
        Some(off),
        "the clicked switch fades all the way off"
    );
}
