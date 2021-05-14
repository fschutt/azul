//! Same as TextInput, but only allows a number

use azul::str::String as AzString;
use azul::callbacks::{RefAny, CallbackInfo, Update};
use azul::vec::NodeDataInlineCssPropertyVec;
use azul::dom::Dom;
use core::ops::Deref;
use core::ops::DerefMut;

use crate::text_input::{
    TextInput, TextInputState,
    OnTextInputReturn,
    TextInputCallback, VirtualKeyDownCallback, OnFocusLostCallback,
    TextInputCallbackFn, VirtualKeyDownCallbackFn, OnFocusLostCallbackFn,
    TextInputStateWrapper, TextInputValid
};

pub type NumberInputCallback = extern "C" fn(&mut RefAny, &NumberInputState, &mut CallbackInfo) -> Update;

pub struct NumberInputCallbackFn {
    pub cb: NumberInputCallback,
}

impl_callback!(NumberInputCallbackFn);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct NumberInput {
    pub text_input: TextInput,
    pub state: NumberInputStateWrapper,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct NumberInputStateWrapper {
    pub inner: NumberInputState,
    pub on_value_change: Option<(NumberInputCallbackFn, RefAny)>,
}

#[derive(Debug, Clone, PartialEq)]
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

    pub fn on_text_input(mut self, callback: TextInputCallback, data: RefAny) -> Self {
        self.text_input.state.on_text_input = Some((TextInputCallbackFn { cb: callback }, data));
        self
    }

    pub fn on_virtual_key_down(mut self, callback: VirtualKeyDownCallback, data: RefAny) -> Self {
        self.text_input.state.on_virtual_key_down = Some((VirtualKeyDownCallbackFn { cb: callback }, data));
        self
    }

    pub fn on_focus_lost(mut self, callback: OnFocusLostCallback, data: RefAny) -> Self {
        self.text_input.state.on_focus_lost = Some((OnFocusLostCallbackFn { cb: callback }, data));
        self
    }

    pub fn placeholder_style(mut self, style: NodeDataInlineCssPropertyVec) -> Self {
        self.text_input.placeholder_style = style;
        self
    }

    pub fn container_style(mut self, style: NodeDataInlineCssPropertyVec) -> Self {
        self.text_input.container_style = style;
        self
    }

    pub fn label_style(mut self, style: NodeDataInlineCssPropertyVec) -> Self {
        self.text_input.label_style = style;
        self
    }

    // Function called when the input has been parsed as a number
    pub fn on_value_change(mut self, callback: NumberInputCallback, data: RefAny) -> Self {
        self.state.on_value_change = Some((NumberInputCallbackFn { cb: callback }, data));
        self
    }

    pub fn dom(mut self) -> Dom {

        let number_string = format!("{}", self.state.inner.number);
        self.text_input.state.inner.text = number_string.chars().collect();

        let state = RefAny::new(self.state);

        self.text_input
        .on_text_input(validate_text_input, state.clone())
        .dom()
    }
}

extern "C" fn validate_text_input(data: &mut RefAny, state: &TextInputState, info: &mut CallbackInfo) -> OnTextInputReturn {

    let mut data = match data.downcast_mut::<NumberInputStateWrapper>() {
        Some(s) => s,
        None => return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes
        },
    };

    let validated_input: String = state.text.iter().collect();
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
            Some((f, d)) => (f.cb)(d, &inner, info),
            None => Update::DoNothing,
        }
    };

    OnTextInputReturn {
        update: result,
        valid: TextInputValid::Yes
    }
}