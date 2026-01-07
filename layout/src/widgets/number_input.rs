//! Same as TextInput, but only allows a number

use std::string::String;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditionsVec,
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::text_input::{
        OnTextInputReturn, TextInput, 
        TextInputOnTextInputCallback, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallback, TextInputOnVirtualKeyDownCallbackType, 
        TextInputOnFocusLostCallback, TextInputOnFocusLostCallbackType,
        TextInputState, TextInputValid,
    },
};

pub type NumberInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, NumberInputState) -> Update;
impl_widget_callback!(
    NumberInputOnValueChange,
    OptionNumberInputOnValueChange,
    NumberInputOnValueChangeCallback,
    NumberInputOnValueChangeCallbackType
);

pub type NumberInputOnFocusLostCallbackType =
    extern "C" fn(RefAny, CallbackInfo, NumberInputState) -> Update;
impl_widget_callback!(
    NumberInputOnFocusLost,
    OptionNumberInputOnFocusLost,
    NumberInputOnFocusLostCallback,
    NumberInputOnFocusLostCallbackType
);

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInput {
    pub number_input_state: NumberInputStateWrapper,
    pub text_input: TextInput,
    pub style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInputStateWrapper {
    pub inner: NumberInputState,
    pub on_value_change: OptionNumberInputOnValueChange,
    pub on_focus_lost: OptionNumberInputOnFocusLost,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInputState {
    pub previous: f32,
    pub number: f32,
    pub min: f32,
    pub max: f32,
}

impl Default for NumberInputState {
    fn default() -> Self {
        Self {
            previous: 0.0,
            number: 0.0,
            min: 0.0,
            max: core::f32::MAX,
        }
    }
}

impl NumberInput {
    pub fn create(input: f32) -> Self {
        Self {
            number_input_state: NumberInputStateWrapper {
                inner: NumberInputState {
                    number: input,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn set_on_text_input<C: Into<TextInputOnTextInputCallback>>(&mut self, refany: RefAny, callback: C) {
        self.text_input.set_on_text_input(refany, callback);
    }

    pub fn with_on_text_input<C: Into<TextInputOnTextInputCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_text_input(refany, callback);
        self
    }

    pub fn set_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input.set_on_virtual_key_down(refany, callback);
    }

    pub fn with_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_virtual_key_down(refany, callback);
        self
    }

    pub fn set_placeholder_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.placeholder_style = style;
    }

    pub fn with_placeholder_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_placeholder_style(style);
        self
    }

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.container_style = style;
    }

    pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn set_label_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.label_style = style;
    }

    pub fn with_label_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_label_style(style);
        self
    }

    // Function called when the input has been parsed as a number
    pub fn set_on_value_change<C: Into<NumberInputOnValueChangeCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.number_input_state.on_value_change = Some(NumberInputOnValueChange {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_value_change<C: Into<NumberInputOnValueChangeCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_value_change(refany, callback);
        self
    }

    pub fn set_on_focus_lost<C: Into<NumberInputOnFocusLostCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.number_input_state.on_focus_lost = Some(NumberInputOnFocusLost {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_focus_lost<C: Into<NumberInputOnFocusLostCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_focus_lost(refany, callback);
        self
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {
        let number_string = format!("{}", self.number_input_state.inner.number);
        self.text_input.text_input_state.inner.text = number_string
            .chars()
            .map(|s| s as u32)
            .collect::<Vec<_>>()
            .into();

        let state = RefAny::new(self.number_input_state);

        self.text_input
            .set_on_text_input(state.clone(), validate_text_input as TextInputOnTextInputCallbackType);
        self.text_input.set_on_focus_lost(state, on_focus_lost as TextInputOnFocusLostCallbackType);
        self.text_input.dom()
    }
}

extern "C" fn on_focus_lost(
    mut refany: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    let mut refany = match refany.downcast_mut::<NumberInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = {
        let number_input = &mut *refany;
        let onfocuslost = &mut number_input.on_focus_lost;
        let inner = number_input.inner.clone();

        match onfocuslost.as_mut() {
            Some(NumberInputOnFocusLost { callback, refany }) => {
                (callback.cb)(refany.clone(), info.clone(), inner)
            }
            None => Update::DoNothing,
        }
    };

    result
}

extern "C" fn validate_text_input(
    mut refany: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let mut refany = match refany.downcast_mut::<NumberInputStateWrapper>() {
        Some(s) => s,
        None => {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        }
    };

    let validated_input: String = state
        .text
        .iter()
        .filter_map(|c| core::char::from_u32(*c))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();

    let validated_f32 = match validated_input.parse::<f32>() {
        Ok(s) => s,
        Err(_) => {
            // do not re-layout the entire screen,
            // but don't handle the character
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::No,
            };
        }
    };

    let result = {
        let number_input = &mut *refany;
        let onvaluechange = &mut number_input.on_value_change;
        let inner = &mut number_input.inner;

        inner.previous = inner.number;
        inner.number = validated_f32;
        let inner_clone = inner.clone();

        match onvaluechange.as_mut() {
            Some(NumberInputOnValueChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info.clone(), inner_clone)
            }
            None => Update::DoNothing,
        }
    };

    OnTextInputReturn {
        update: result,
        valid: TextInputValid::Yes,
    }
}
