//! Text input (demonstrates two-way data binding)

use {
    traits::Layout,
    dom::{Dom, On, NodeType, UpdateScreen},
    window::{FakeWindow, WindowEvent},
    prelude::{VirtualKeyCode},
    default_callbacks::{StackCheckedPointer, DefaultCallback, DefaultCallbackId},
    app_state::AppStateNoData,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {
    on_key_down_callback: Option<DefaultCallbackId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInputState {
    pub text: String,
}

impl TextInputState {
    pub fn new<S: Into<String>>(input: S) -> Self {
        Self {
            text: input.into(),
        }
    }
}

struct TextInputCallback<'a> {
    ptr: &'a TextInputState,
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { on_key_down_callback: None }
    }

    pub fn bind<T: Layout>(self, window: &mut FakeWindow<T>, field: &TextInputState, data: &T) -> Self {
        let on_key_down_callback = StackCheckedPointer::new(data, field).and_then(|ptr|{
            Some(window.add_callback(ptr, DefaultCallback(TextInputState::on_key_down_private)))
        });

        Self {
            on_key_down_callback,
            .. self
        }
    }

    pub fn dom<T: Layout>(&self, field: &TextInputState) -> Dom<T> {

        let mut parent_div = Dom::new(NodeType::Div).with_class("__azul-native-input-text");

        if let Some(on_key_down_callback) = self.on_key_down_callback {
            parent_div.add_default_callback_id(On::KeyDown, on_key_down_callback);
        } else {
            parent_div.enable_hit_testing(On::KeyDown);
        }

        parent_div.with_child(Dom::new(NodeType::Label(field.text.clone())).with_class("__azul-native-input-text-label"))
    }
}

impl TextInputState {

    fn on_key_down_private<T: Layout>(data: &StackCheckedPointer<T>, app_state_no_data: AppStateNoData<T>, window_event: WindowEvent<T>)
    -> UpdateScreen
    {
        unsafe { data.invoke_mut(Self::on_key_down, app_state_no_data, window_event) }
    }

    pub fn on_key_down<T: Layout>(&mut self, app_state_no_data: AppStateNoData<T>, event: WindowEvent<T>)
    -> UpdateScreen
    {
        let keyboard_state = app_state_no_data.windows[event.window].get_keyboard_state();

        match keyboard_state.latest_virtual_keycode {
            Some(VirtualKeyCode::Back) => { self.text.pop(); },
            Some(key) => {
                use window_state::virtual_key_code_to_char;
                if let Some(key) = virtual_key_code_to_char(key) {
                    // This next unwrap is safe as there will always be one character in the iterator.
                    self.text.push(if keyboard_state.shift_down { key.to_uppercase().next().unwrap() } else { key });
                }
            },
            None => { },
        }

        UpdateScreen::Redraw
    }
}

