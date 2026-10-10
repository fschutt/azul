//! `CallbackInfo::set_node_style`: a callback hands a node a new inline
//! stylesheet - a `Css`, the type the node stores its style in
//! (`NodeData::set_style`) - and once the change is applied the node resolves
//! that stylesheet's conditional rules like a node BUILT with it: its
//! `:hover` rule when hovered, its dark twin in dark mode, and the dark
//! `:hover` twin when both hold. Nothing is pinned: `set_css_property`
//! writes one value as a user override, which outranks every one of these
//! rules.
//!
//! The callback runs the way the shell runs it (`invoke_single_callback_at`);
//! its `CallbackChange::SetNodeStyle` lands through the content chokepoint the
//! dll host and the e2e runner both delegate it to.

use std::sync::Arc;

use azul_core::{
    callbacks::Update,
    dom::{DomId, DomNodeId},
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledNodeState},
    window::{OptionDarkLightMode, RawWindowHandle, DarkLightMode},
};
use azul_css::{
    css::Css,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::color::ColorU,
        layout::{LayoutHeight, LayoutWidth},
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
    system::SystemStyle,
};
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo, ExternalSystemCallbacks},
    overlay::ContentDirtyTier,
    window::LayoutWindow,
};

use super::a_replaced_inline_style_follows_the_mode::{
    box_fill, env_pinned, fill, replace, switch_scheme, window, BOX, RED,
};

const GREEN: ColorU = ColorU::rgb(0, 160, 0);
const YELLOW: ColorU = ColorU::rgb(220, 200, 0);
const PURPLE: ColorU = ColorU::rgb(120, 0, 160);
const ORANGE: ColorU = ColorU::rgb(240, 120, 0);

/// The box's new stylesheet: the same 40 x 20 box, green at rest, purple
/// under the pointer, each with a dark twin (yellow, orange).
fn hover_and_dark_style() -> Css {
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(20))),
        CssPropertyWithConditions::simple(fill(GREEN)),
        CssPropertyWithConditions::dark_mode(fill(YELLOW)),
        CssPropertyWithConditions::on_hover(fill(PURPLE)),
        CssPropertyWithConditions::dark_on_hover(fill(ORANGE)),
    ])
    .into()
}

/// The payload: which node to restyle, and with what.
struct Restyle {
    target: DomNodeId,
    style: Css,
}

/// The app's handler: one `set_node_style`, no DOM refresh.
extern "C" fn restyle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((target, style)) = data
        .downcast_ref::<Restyle>()
        .map(|r| (r.target, r.style.clone()))
    else {
        return Update::DoNothing;
    };
    info.set_node_style(target, style);
    Update::DoNothing
}

fn box_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(BOX)),
    }
}

/// Run `restyle` on a click on the box, the way the shell runs a callback,
/// and hand back what it pushed.
fn click_restyle(lw: &mut LayoutWindow, target: DomNodeId, style: Css) -> Vec<CallbackChange> {
    let mut callback = Callback::from_ptr(restyle);
    let mut data = RefAny::new(Restyle { target, style });
    let state = lw.current_window_state.clone();
    let (changes, _update) = lw.invoke_single_callback_at(
        box_id(),
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
    changes
}

/// The background the box resolves to, at rest or hovered, as the cascade
/// answers it (user overrides first - what the display list reads).
fn background(lw: &LayoutWindow, hover: bool) -> Option<ColorU> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    let state = StyledNodeState {
        hover,
        ..StyledNodeState::default()
    };
    let property = sd.get_css_property_cache().get_property(
        &node_data[BOX],
        &BOX,
        &state,
        &CssPropertyType::BackgroundContent,
    )?;
    match property {
        CssProperty::BackgroundContent(v) => match v.get_property()?.as_ref().first()? {
            StyleBackgroundContent::Color(c) => Some(*c),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn a_stylesheet_set_by_a_callback_resolves_its_hover_rule_and_its_dark_twin() {
    if env_pinned() {
        return;
    }
    let mut lw = window();
    assert_eq!(background(&lw, false), Some(RED), "premise: built red by day");
    assert_eq!(background(&lw, true), Some(RED), "premise: no hover rule yet");

    let changes = click_restyle(&mut lw, box_id(), hover_and_dark_style());
    let [CallbackChange::SetNodeStyle {
        dom_id,
        node_id,
        style,
    }] = changes.as_slice()
    else {
        panic!("the callback pushes exactly one node-style change: {changes:?}");
    };
    assert_eq!((*dom_id, *node_id), (DomId::ROOT_ID, BOX), "on the node it named");
    assert_eq!(*style, hover_and_dark_style(), "carrying the stylesheet unchanged");

    assert_eq!(
        replace(&mut lw, style.clone()),
        ContentDirtyTier::RebuildDisplayList,
        "only colours changed: a repaint, not a relayout"
    );
    assert_eq!(box_fill(&lw, 40.0), Some(GREEN), "the new style paints at once");
    assert_eq!(background(&lw, false), Some(GREEN), "by day at rest: the new light face");
    assert_eq!(background(&lw, true), Some(PURPLE), "by day hovered: the new :hover rule");

    switch_scheme(&mut lw, OptionDarkLightMode::Some(DarkLightMode::Dark));
    assert_eq!(background(&lw, false), Some(YELLOW), "by night at rest: the dark twin");
    assert_eq!(
        background(&lw, true),
        Some(ORANGE),
        "by night hovered: the dark :hover twin - no light colour pinned over it"
    );

    switch_scheme(&mut lw, OptionDarkLightMode::None);
    assert_eq!(background(&lw, false), Some(GREEN), "and by day again the light face");
}

#[test]
fn a_callback_restyling_a_node_id_without_a_node_pushes_nothing() {
    let mut lw = window();
    let nowhere = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
    let changes = click_restyle(&mut lw, nowhere, hover_and_dark_style());
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, CallbackChange::SetNodeStyle { .. })),
        "a node id without a node is ignored: {changes:?}"
    );
}
