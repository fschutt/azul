//! A statically transparent element paints nothing.
//!
//! The Tooltip widget hides its tip with an inline `opacity: 0` and reveals it
//! on MouseEnter. The tip painted permanently: no `PushOpacity` group was
//! emitted around it, so the display list drew it fully opaque over the
//! layout below it (AzWidgets' "I am a tooltip!" under "Hover me").

use azul_core::{
    dom::Dom,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn opacities(dom: Dom) -> Vec<f32> {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw.get_layout_result(&azul_core::dom::DomId::ROOT_ID)
        .unwrap()
        .display_list
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayListItem::PushOpacity { opacity, .. } => Some(*opacity),
            _ => None,
        })
        .collect()
}

/// Control: an inline `opacity: 0` on a plain div is wrapped in an opacity
/// group of 0.
#[test]
fn an_inline_opacity_zero_div_is_wrapped_in_an_opacity_group() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("opacity: 0; width: 50px; height: 20px; background-color: red;"),
    );
    let ops = opacities(dom);
    assert!(ops.contains(&0.0), "PushOpacity(0) expected, got {ops:?}");
}

/// The same div, absolutely positioned inside a relative parent (the
/// tooltip's shape).
#[test]
fn an_absolutely_positioned_opacity_zero_div_is_wrapped_in_an_opacity_group() {
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_css("position: relative;").with_child(
            Dom::create_div().with_css(
                "position: absolute; opacity: 0; width: 50px; height: 20px; background-color: \
                 red;",
            ),
        ),
    );
    let ops = opacities(dom);
    assert!(ops.contains(&0.0), "PushOpacity(0) expected, got {ops:?}");
}

/// The widget case: the tooltip's tip starts hidden.
#[test]
fn a_tooltip_tip_is_hidden_until_hovered() {
    let tip = azul_layout::widgets::tooltip::Tooltip::new(
        azul_layout::widgets::button::Button::create("Hover me".into()).dom(),
        "I am a tooltip!".into(),
    )
    .dom();
    let ops = opacities(Dom::create_body().with_child(tip));
    assert!(
        ops.contains(&0.0),
        "the tooltip's tip must be painted inside an opacity-0 group, got {ops:?}"
    );
}
