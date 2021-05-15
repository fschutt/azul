//! Rectangular input that, when clicked, spawns a color dialog

use azul_desktop::css::*;
use azul_desktop::dom::{
    Dom, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec,
    NodeDataInlineCssProperty::Normal
 };
use azul_desktop::callbacks::{Update, CallbackInfo, RefAny};
use azul_desktop::css::AzString;
use std::vec::Vec;

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct ColorInput {
    pub state: ColorInputStateWrapper,
    pub style: NodeDataInlineCssPropertyVec,
}

pub type ColorInputOnValueChangeCallbackType = extern "C" fn(&mut RefAny, &ColorInputState, &mut CallbackInfo) -> Update;

#[repr(C)]
pub struct ColorInputOnValueChangeCallback {
    pub cb: ColorInputOnValueChangeCallbackType,
}

impl_callback!(ColorInputOnValueChangeCallback);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ColorInputStateWrapper {
    pub inner: ColorInputState,
    pub title: AzString,
    pub on_value_change: OptionColorInputOnValueChange,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ColorInputOnValueChange {
    pub data: RefAny,
    pub callback: ColorInputOnValueChangeCallback,
}

impl_option!(ColorInputOnValueChange, OptionColorInputOnValueChange, copy = false, [Debug, Clone, PartialEq, PartialOrd]);

impl Default for ColorInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: ColorInputState::default(),
            title: AzString::from_const_str("Pick color"),
            on_value_change: None.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct ColorInputState {
    pub color: ColorU,
}

impl Default for ColorInputState {
    fn default() -> Self {
        Self {
            color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }
        }
    }
}

static DEFAULT_COLOR_INPUT_STYLE: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::const_min_width(LayoutMinWidth::const_px(15))),
    Normal(CssProperty::const_min_height(LayoutMinHeight::const_px(15))),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
];

impl ColorInput {

    #[inline]
    pub fn new(color: ColorU) -> Self {
        Self {
            state: ColorInputStateWrapper {
                inner: ColorInputState {
                    color,
                    .. Default::default()
                },
                .. Default::default()
            },
            style: NodeDataInlineCssPropertyVec::from_const_slice(DEFAULT_COLOR_INPUT_STYLE),
        }
    }

    #[inline]
    pub fn set_on_value_change(&mut self, data: RefAny, callback: ColorInputOnValueChangeCallbackType) {
        self.state.on_value_change = Some(ColorInputOnValueChange {
            callback: ColorInputOnValueChangeCallback { cb: callback },
            data
        }).into();
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn dom(mut self) -> Dom {

        use azul_desktop::callbacks::Callback;
        use azul_desktop::dom::{
            EventFilter, HoverEventFilter,
            IdOrClass::Class, CallbackData,
        };

        let mut style = self.style.into_library_owned_vec();
        style.push(Normal(CssProperty::const_background_content(vec![
            StyleBackgroundContent::Color(self.state.inner.color)
        ].into())));

        Dom::div()
        .with_ids_and_classes(vec![Class("__azul_native_color_input".into())].into())
        .with_inline_css_props(style.into())
        .with_callbacks(vec![
            CallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                data: RefAny::new(self.state),
                callback: Callback { cb: on_color_input_clicked }
            }
        ].into())
    }
}

extern "C" fn on_color_input_clicked(data: &mut RefAny, mut info: CallbackInfo) -> Update {

    use azul_desktop::dialogs::color_picker_dialog;

    let mut color_input = match data.downcast_mut::<ColorInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // open the color picker dialog
    let new_color = match color_picker_dialog(
        color_input.title.as_str(),
        Some(color_input.inner.color).into()
    ) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Update the color in the data and the screen
    color_input.inner.color = new_color;
    info.set_css_property(info.get_hit_node(), CssProperty::const_background_content(
        vec![StyleBackgroundContent::Color(new_color)].into(),
    ));

    let result = {
        let color_input = &mut *color_input;
        let onvaluechange = &mut color_input.on_value_change;
        let inner = &color_input.inner;

        match onvaluechange.as_mut() {
            Some(ColorInputOnValueChange { callback, data }) => (callback.cb)(data, &inner, &mut info),
            None => Update::DoNothing,
        }
    };

    result
}