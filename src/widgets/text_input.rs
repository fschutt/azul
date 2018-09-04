//! Test text input to test two-way data binding. Do not use!

use {
    traits::{Layout, DefaultCallbackFn},
    dom::{Dom, On, NodeType},
    window::{FakeWindow, WindowInfo, WindowEvent},
    prelude::{VirtualKeyCode},
    default_callbacks::StackCheckedPointer,
    /*TODO: Replace this with the node hash*/
    id_tree::NodeId,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {

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

    fn get_callback_fn<U: Layout>(&self) -> fn(&StackCheckedPointer<U>) {
        update_text_field
    }
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { }
    }

    pub fn bind<T: Layout>(self, window: &mut FakeWindow<T>, field: &TextInputOutcome, data: &T) -> Self {

        window.push_callback(
            TextInputCallback { ptr: field }, 
            data,
            NodeId::new(0), /* TODO: replace with node hash */
            On::MouseOver);

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
    pub fn update<T: Layout>(&mut self, windows: &[FakeWindow<T>], event: &WindowEvent) {

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

fn update_text_field<T: Layout>(data: &StackCheckedPointer<T>) {
    
    fn update_text_field_inner(data: &mut TextInputOutcome) {
        println!("updating text field: {:?}", data);
        data.text.pop();
    }

    unsafe { data.invoke_mut(update_text_field_inner) };
}

/*

.with_callback(On::KeyDown, Callback(update_text_field))

fn update_text_field(app_state: &mut AppState<TestCrudApp>, event: WindowEvent) -> UpdateScreen {
    app_state.data.modify(|state| state.text_input.update(&app_state.windows, &event));
    UpdateScreen::Redraw
}
*/