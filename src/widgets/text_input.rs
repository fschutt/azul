//! Text input (demonstrates two-way data binding)

use {
    traits::{Layout, DefaultCallbackFn},
    dom::{Dom, On, NodeType, UpdateScreen},
    window::{FakeWindow, WindowEvent},
    prelude::{VirtualKeyCode},
    default_callbacks::{StackCheckedPointer, DefaultCallback, DefaultCallbackId},
    app_state::AppStateNoData,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {
    default_callback_id: Option<DefaultCallbackId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInputOutcome {
    pub text: String,
}

impl TextInputOutcome {
    pub fn new<S: Into<String>>(input: S) -> Self {
        Self {
            text: input.into(),
        }
    }
}

struct TextInputCallback<'a> {
    ptr: &'a TextInputOutcome,
}

impl<'a> DefaultCallbackFn for TextInputCallback<'a> {
    type Outcome = TextInputOutcome;

    fn get_callback_ptr(&self) -> &Self::Outcome {
        self.ptr
    }

    fn get_callback_fn<U: Layout>(&self) -> DefaultCallback<U> {
        DefaultCallback(update_text_field)
    }
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { default_callback_id: None }
    }

    pub fn bind<T: Layout>(self, window: &mut FakeWindow<T>, field: &TextInputOutcome, data: &T) -> Self {
        let default_callback_id = window.push_callback(TextInputCallback { ptr: field }, data);

        Self {
            default_callback_id: Some(default_callback_id),
            .. self
        }
    }

    pub fn dom<T: Layout>(&self, field: &TextInputOutcome) -> Dom<T> {

        let mut parent_div = Dom::new(NodeType::Div).with_id("input_field");

        if let Some(default_callback_id) = self.default_callback_id {
            parent_div.push_default_callback_id(On::MouseOver, default_callback_id);
        }

        parent_div.with_child(Dom::new(NodeType::Label(field.text.clone())).with_id("label"))
    }
}

fn update_text_field<T: Layout>(data: &StackCheckedPointer<T>, app_state_no_data: AppStateNoData<T>, window_event: WindowEvent)
-> UpdateScreen
{
    unsafe { data.invoke_mut(update_text_field_inner, app_state_no_data, window_event) }
}

fn update_text_field_inner<T: Layout>(data: &mut TextInputOutcome, app_state_no_data: AppStateNoData<T>, event: WindowEvent)
-> UpdateScreen
{
    let keyboard_state = app_state_no_data.windows[event.window].get_keyboard_state();

    if keyboard_state.current_virtual_keycodes.contains(&VirtualKeyCode::Back) {
        data.text.pop();
    } else {
        let mut keys = keyboard_state.current_keys.iter().cloned().collect::<String>();
        if keyboard_state.shift_down {
            keys = keys.to_uppercase();
        }
        data.text += &keys;
    }
    UpdateScreen::Redraw
}