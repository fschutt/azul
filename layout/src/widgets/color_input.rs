//! Rectangular input that displays a color and invokes a callback when clicked

use azul_core::{
    callbacks::Update,
    dom::Dom,
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
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

/// Rectangular input that displays a color and triggers a callback when clicked.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct ColorInput {
    pub color_input_state: ColorInputStateWrapper,
    pub style: CssPropertyWithConditionsVec,
}

/// Callback function type invoked when the color input value changes.
pub type ColorInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ColorInputState) -> Update;
impl_widget_callback!(
    ColorInputOnValueChange,
    OptionColorInputOnValueChange,
    ColorInputOnValueChangeCallback,
    ColorInputOnValueChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ColorInputOnValueChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: COLOR_INPUT_ON_VALUE_CHANGE_INVOKER,
    invoker_ty:     AzColorInputOnValueChangeCallbackInvoker,
    thunk_fn:       az_color_input_on_value_change_callback_thunk,
    setter_fn:      AzApp_setColorInputOnValueChangeCallbackInvoker,
    from_handle_fn: AzColorInputOnValueChangeCallback_createFromHostHandle,
    extra_args:     [ state: ColorInputState ],
}

/// Wrapper around [`ColorInputState`] that includes a title and an optional value-change callback.
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

/// Holds the current color value of a [`ColorInput`] widget.
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

static DEFAULT_COLOR_INPUT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

impl ColorInput {
    /// Creates a new `ColorInput` displaying the given color.
    #[inline]
    #[must_use]
    pub fn create(color: ColorU) -> Self {
        Self {
            color_input_state: ColorInputStateWrapper {
                inner: ColorInputState { color },
                ..Default::default()
            },
            style: CssPropertyWithConditionsVec::from_const_slice(DEFAULT_COLOR_INPUT_STYLE),
        }
    }

    /// Sets the callback invoked when the color value changes.
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

    /// Builder-style method to set the value-change callback.
    #[inline]
    #[must_use]
    pub fn with_on_value_change<C: Into<ColorInputOnValueChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_value_change(data, callback);
        self
    }

    /// Replaces `self` with a default `ColorInput` and returns the previous value.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this `ColorInput` into a styled [`Dom`] node with a click callback.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter, IdOrClass::Class},
        };

        let mut style = self.style.into_library_owned_vec();
        style.push(CssPropertyWithConditions::simple(
            CssProperty::const_background_content(
                vec![StyleBackgroundContent::Color(
                    self.color_input_state.inner.color,
                )]
                .into(),
            ),
        ));

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul_native_color_input".into())].into())
            .with_css_props(style.into())
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

    // No built-in color picker dialog — the on_value_change callback
    // receives the current color so the caller can open their own picker.
    let color_input = &mut *color_input;
    let onvaluechange = &mut color_input.on_value_change;
    let inner = color_input.inner.clone();

    match onvaluechange.as_mut() {
        Some(ColorInputOnValueChange {
            callback,
            refany: data,
        }) => (callback.cb)(data.clone(), info.clone(), inner),
        None => Update::DoNothing,
    }
}
