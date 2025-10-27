//! Shared Client-Side Decorations (CSD) for Linux backends.

use azul_core::{
    callbacks::{Update, CoreCallbackData, CoreCallback},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec},
    refany::RefAny,
    window::{WindowFrame, WindowFlags},
};
use azul_css::props::{
    basic::color::ColorU,
    layout::*,
    property::CssProperty,
    style::*,
};
use azul_layout::callbacks::CallbackInfo;

const TITLE_BAR_HEIGHT: f32 = 30.0;
const BUTTON_WIDTH: f32 = 45.0;
const TITLE_BAR_BG: ColorU = ColorU { r: 45, g: 45, b: 45, a: 255 };
const BUTTON_BG: ColorU = ColorU { r: 50, g: 50, b: 50, a: 255 };
const BUTTON_CLOSE_HOVER_BG: ColorU = ColorU { r: 220, g: 70, b: 70, a: 255 };
const BUTTON_HOVER_BG: ColorU = ColorU { r: 70, g: 70, b: 70, a: 255 };
const TEXT_COLOR: ColorU = ColorU { r: 220, g: 220, b: 220, a: 255 };

/// State for tracking hover on decoration buttons.
#[derive(Default, Clone)]
pub struct DecorationsState {
    pub is_dragging: bool,
    pub close_button_hover: bool,
    pub maximize_button_hover: bool,
    pub minimize_button_hover: bool,
}

/// Renders the DOM for the title bar.
pub fn render_decorations(title: &str, state: &DecorationsState) -> Dom {
    fn button_style(hover_bg: ColorU, is_hovered: bool) -> Vec<NodeDataInlineCssProperty> {
        vec![
            CssProperty::Width(LayoutWidth::Px(BUTTON_WIDTH.into()).into()).into(),
            CssProperty::Height(LayoutHeight::Px(TITLE_BAR_HEIGHT.into()).into()).into(),
            CssProperty::BackgroundContent(
                if is_hovered { vec![StyleBackgroundContent::Color(hover_bg)].into() }
                else { vec![StyleBackgroundContent::Color(BUTTON_BG)].into() }
            ).into(),
            CssProperty::JustifyContent(LayoutJustifyContent::Center.into()).into(),
            CssProperty::AlignItems(LayoutAlignItems::Center.into()).into(),
        ]
    }

    Dom::div()
        .with_id("csd-title-bar")
        .with_inline_css_props(vec![
            CssProperty::Width(LayoutWidth::Percentage(100.0).into()).into(),
            CssProperty::Height(LayoutHeight::Px(TITLE_BAR_HEIGHT.into()).into()).into(),
            CssProperty::BackgroundContent(vec![StyleBackgroundContent::Color(TITLE_BAR_BG)].into()).into(),
            CssProperty::FlexDirection(LayoutFlexDirection::Row.into()).into(),
            CssProperty::AlignItems(LayoutAlignItems::Center.into()).into(),
        ].into())
        .with_callback(CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            data: RefAny::new(()), // Simple marker for drag start
            callback: CoreCallback { cb: on_title_bar_drag_start as usize },
        })
        .with_children(DomVec::from_vec(vec![
            Dom::div() // Title
                .with_inline_css_props(vec![
                    CssProperty::FlexGrow(1.0.into()).into(),
                    CssProperty::PaddingLeft(10.0.into()).into(),
                    CssProperty::TextColor(TEXT_COLOR.into()).into(),
                ].into())
                .with_child(Dom::text(title.into())),
            Dom::div() // Minimize
                .with_inline_css_props(button_style(BUTTON_HOVER_BG, state.minimize_button_hover).into())
                .with_child(Dom::text("-".into()))
                .with_callback(CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    data: RefAny::new(()),
                    callback: CoreCallback { cb: on_minimize_click as usize },
                }),
            Dom::div() // Maximize
                .with_inline_css_props(button_style(BUTTON_HOVER_BG, state.maximize_button_hover).into())
                .with_child(Dom::text("[ ]".into()))
                .with_callback(CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    data: RefAny::new(()),
                    callback: CoreCallback { cb: on_maximize_click as usize },
                }),
            Dom::div() // Close
                .with_inline_css_props(button_style(BUTTON_CLOSE_HOVER_BG, state.close_button_hover).into())
                .with_child(Dom::text("X".into()))
                .with_callback(CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    data: RefAny::new(()),
                    callback: CoreCallback { cb: on_close_click as usize },
                }),
        ]))
}

// -- Callbacks that modify the WindowState --

extern "C" fn on_close_click(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut state = info.get_current_window_state();
    state.flags.close_requested = true;
    info.set_window_state(state);
    Update::DoNothing
}

extern "C" fn on_minimize_click(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut state = info.get_current_window_state();
    state.flags.frame = WindowFrame::Minimized;
    info.set_window_state(state);
    Update::DoNothing
}

extern "C" fn on_maximize_click(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut state = info.get_current_window_state();
    state.flags.frame = if state.flags.frame == WindowFrame::Maximized {
        WindowFrame::Normal
    } else {
        WindowFrame::Maximized
    };
    info.set_window_state(state);
    Update::DoNothing
}

// This callback is just a marker. The native event loop will see a mousedown
// on the title bar and initiate a move.
extern "C" fn on_title_bar_drag_start(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    // No-op. The presence of this callback on the hit-tested node is enough.
    Update::DoNothing
}
