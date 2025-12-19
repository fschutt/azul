//! Rectangular input that, when clicked, spawns a color dialog

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{
        Dom, NodeDataInlineCssProperty, NodeDataInlineCssProperty::Normal,
        NodeDataInlineCssPropertyVec,
    },
    refany::RefAny,
};
use azul_css::{
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct ColorInput {
    pub color_input_state: ColorInputStateWrapper,
    pub style: NodeDataInlineCssPropertyVec,
}

pub type ColorInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ColorInputState) -> Update;
impl_widget_callback!(
    ColorInputOnValueChange,
    OptionColorInputOnValueChange,
    ColorInputOnValueChangeCallback,
    ColorInputOnValueChangeCallbackType
);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ColorInputStateWrapper {
    pub inner: ColorInputState,
    pub title: AzString,
    pub on_value_change: OptionColorInputOnValueChange,
}

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
            },
        }
    }
}

static DEFAULT_COLOR_INPUT_STYLE: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Block)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_width(LayoutWidth::const_px(14))),
    Normal(CssProperty::const_height(LayoutHeight::const_px(14))),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
];

impl ColorInput {
    #[inline]
    pub fn create(color: ColorU) -> Self {
        Self {
            color_input_state: ColorInputStateWrapper {
                inner: ColorInputState {
                    color,
                    ..Default::default()
                },
                ..Default::default()
            },
            style: NodeDataInlineCssPropertyVec::from_const_slice(DEFAULT_COLOR_INPUT_STYLE),
        }
    }

    #[inline]
    pub fn set_on_value_change<I: Into<ColorInputOnValueChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: I,
    ) {
        self.color_input_state.on_value_change = Some(ColorInputOnValueChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    pub fn with_on_value_change(
        mut self,
        data: RefAny,
        callback: ColorInputOnValueChangeCallbackType,
    ) -> Self {
        self.set_on_value_change(data, callback);
        self
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter, IdOrClass::Class},
        };

        let mut style = self.style.into_library_owned_vec();
        style.push(Normal(CssProperty::const_background_content(
            vec![StyleBackgroundContent::Color(self.color_input_state.inner.color)].into(),
        )));

        Dom::new_div()
            .with_ids_and_classes(vec![Class("__azul_native_color_input".into())].into())
            .with_inline_css_props(style.into())
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    refany: RefAny::new(self.color_input_state),
                    callback: CoreCallback {
                        cb: on_color_input_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                }]
                .into(),
            )
    }
}

extern "C" fn on_color_input_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut color_input = match data.downcast_mut::<ColorInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Color picker dialog is not available in azul_layout
    // The user must provide their own color picker callback via on_value_change
    // For now, just trigger the callback with the current color
    let result = {
        let color_input = &mut *color_input;
        let onvaluechange = &mut color_input.on_value_change;
        let inner = color_input.inner.clone();

        match onvaluechange.as_mut() {
            Some(ColorInputOnValueChange { callback, refany: data }) => {
                (callback.cb)(data.clone(), info.clone(), inner)
            }
            None => Update::DoNothing,
        }
    };

    result
}
