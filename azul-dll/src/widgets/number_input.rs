//! Same as TextInput, but only allows a number

use azul_desktop::css::AzString;
use azul_desktop::callbacks::{RefAny, CallbackInfo, Update};
use azul_desktop::dom::{Dom, NodeDataInlineCssPropertyVec};
use core::ops::Deref;
use core::ops::DerefMut;
use std::string::String;
use azul_desktop::css::{impl_option, impl_option_inner};

use crate::widgets::text_input::{
    TextInput, TextInputState,
    OnTextInputReturn,
    TextInputStateWrapper, TextInputValid,
    TextInputOnFocusLostCallbackType, TextInputOnVirtualKeyDownCallbackType,
    TextInputOnTextInputCallbackType,
};

pub type NumberInputOnValueChangeCallbackType = extern "C" fn(&mut RefAny, &NumberInputState, &mut CallbackInfo) -> Update;

#[repr(C)]
pub struct NumberInputOnValueChangeCallback {
    pub cb: NumberInputOnValueChangeCallbackType,
}

impl_callback!(NumberInputOnValueChangeCallback);

pub type NumberInputOnFocusLostCallbackType = extern "C" fn(&mut RefAny, &NumberInputState, &mut CallbackInfo) -> Update;

#[repr(C)]
pub struct NumberInputOnFocusLostCallback {
    pub cb: NumberInputOnFocusLostCallbackType,
}

impl_callback!(NumberInputOnFocusLostCallback);

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInput {
    pub text_input: TextInput,
    pub state: NumberInputStateWrapper,
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
pub struct NumberInputOnFocusLost {
    pub data: RefAny,
    pub callback: NumberInputOnFocusLostCallback,
}


#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInputOnValueChange {
    pub data: RefAny,
    pub callback: NumberInputOnValueChangeCallback,
}

impl_option!(NumberInputOnFocusLost, OptionNumberInputOnFocusLost, copy = false, [Debug, Clone, PartialEq]);
impl_option!(NumberInputOnValueChange, OptionNumberInputOnValueChange, copy = false, [Debug, Clone, PartialEq]);

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

    pub fn new(input: f32) -> Self {
        Self {
            state: NumberInputStateWrapper {
                inner: NumberInputState {
                    number: input,
                    .. Default::default()
                },
                .. Default::default()
            },
            .. Default::default()
        }
    }

    pub fn set_on_text_input(&mut self, data: RefAny, callback: TextInputOnTextInputCallbackType) {
        self.text_input.set_on_text_input(data, callback);
    }

    pub fn set_on_virtual_key_down(&mut self, data: RefAny, callback: TextInputOnVirtualKeyDownCallbackType) {
        self.text_input.set_on_virtual_key_down(data, callback);
    }

    pub fn set_placeholder_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.text_input.placeholder_style = style;
    }

    pub fn set_container_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.text_input.container_style = style;
    }

    pub fn set_label_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.text_input.label_style = style;
    }

    // Function called when the input has been parsed as a number
    pub fn set_on_value_change(&mut self, data: RefAny, callback: NumberInputOnValueChangeCallbackType) {
        self.state.on_value_change = Some(NumberInputOnValueChange {
            callback: NumberInputOnValueChangeCallback { cb: callback },
            data
        }).into();
    }

    pub fn set_on_focus_lost(&mut self, data: RefAny, callback: NumberInputOnFocusLostCallbackType) {
        self.state.on_focus_lost = Some(NumberInputOnFocusLost {
            callback: NumberInputOnFocusLostCallback { cb: callback },
            data
        }).into();
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {

        let number_string = format!("{}", self.state.inner.number);
        self.text_input.state.inner.text = number_string.chars()
        .map(|s| s as u32).collect::<Vec<_>>().into();

        let state = RefAny::new(self.state);

        self.text_input.set_on_text_input(state.clone(), validate_text_input);
        self.text_input.set_on_focus_lost(state, on_focus_lost);
        self.text_input.dom()
    }
}

extern "C" fn on_focus_lost(data: &mut RefAny, state: &TextInputState, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NumberInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = {
        let number_input = &mut *data;
        let onfocuslost = &mut number_input.on_focus_lost;
        let inner = &number_input.inner;

        match onfocuslost.as_mut() {
            Some(NumberInputOnFocusLost { callback, data }) => (callback.cb)(data, &inner, info),
            None => Update::DoNothing,
        }
    };

    result
}

extern "C" fn validate_text_input(data: &mut RefAny, state: &TextInputState, info: &mut CallbackInfo) -> OnTextInputReturn {

    let mut data = match data.downcast_mut::<NumberInputStateWrapper>() {
        Some(s) => s,
        None => return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes
        },
    };

    let validated_input: String = state.text.iter()
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
        let number_input = &mut *data;
        let onvaluechange = &mut number_input.on_value_change;
        let inner = &number_input.inner;

        match onvaluechange.as_mut() {
            Some(NumberInputOnValueChange { callback, data }) => (callback.cb)(data, &inner, info),
            None => Update::DoNothing,
        }
    };

    OnTextInputReturn {
        update: result,
        valid: TextInputValid::Yes
    }
}