//! Test text input to test two-way data binding. Do not use!

use {
    traits::Layout,
    dom::{Dom, NodeType},
    window::{FakeWindow, WindowEvent},
    prelude::{VirtualKeyCode},
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {

}

pub struct TextInputOutcome {
    pub text: String,
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { }
    }

    pub fn bind(self, field: &TextInputOutcome) -> Self {
        let mem_location = field as * const _ as usize;
        println!("binding text field to: 0x{:x}", mem_location);
        self
    }

    pub fn dom<T: Layout>(&self, field: &TextInputOutcome) -> Dom<T> {
        Dom::new(NodeType::Div)
        .with_id("input_field")
        .with_child(
            Dom::new(NodeType::Label(field.text.clone()))
            .with_id("label"))
    }
}

impl TextInputOutcome {
    /// Updates the text input, given an event
    pub fn update(&mut self, windows: &[FakeWindow], event: &WindowEvent) {

        let keyboard_state = windows[event.window].get_keyboard_state();

        if keyboard_state.current_virtual_keycodes.contains(&VirtualKeyCode::Back) {
            self.text.pop();
        } else {
            let mut keys = keyboard_state.current_keys.iter().cloned().collect::<String>();
            if keyboard_state.shift_down {
                keys = keys.to_uppercase();
            }
            self.text += &keys;
        }
    }
}