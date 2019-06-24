//! Text input (demonstrates two-way data binding)

use std::ops::Range;
use azul_core::{
    callbacks::{Redraw, DontRedraw},
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    window::{FakeWindow, VirtualKeyCode},
    callbacks::{
        StackCheckedPointer, DefaultCallbackInfo,
        DefaultCallbackInfoUnchecked, DefaultCallbackId, CallbackReturn,
    },
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {
    on_text_input_callback: Option<(DefaultCallbackId, DefaultCallbackId)>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInputState {
    pub text: String,
    pub selection: Option<Selection>,
    pub cursor: usize,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Selection {
    All,
    FromTo(Range<usize>),
}

impl Default for TextInputState {
    fn default() -> Self {
        TextInputState {
            text: String::new(),
            selection: None,
            cursor: 0,
        }
    }
}

impl TextInputState {
    pub fn new<S: Into<String>>(input: S) -> Self {
        let input_str: String = input.into();
        let len = input_str.len();
        Self {
            text: input_str,
            selection: None,
            cursor: len,
        }
    }
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { on_text_input_callback: None }
    }

    pub fn bind<T>(self, window: &mut FakeWindow<T>, field: &TextInputState, data: &T) -> Self {
        let ptr = StackCheckedPointer::new(data, field);
        let on_text_input_callback = ptr.map(|ptr|{(
            window.add_default_callback(text_input_on_text_input_private, ptr),
            window.add_default_callback(text_input_on_virtual_key_down_private, ptr),
        )});

        Self {
            on_text_input_callback,
            .. self
        }
    }

    pub fn dom<T>(&self, field: &TextInputState) -> Dom<T> {

        let mut parent_div =
            Dom::div()
            .with_class("__azul-native-input-text")
            .with_tab_index(TabIndex::Auto);

        if let Some((text_input_callback, vk_callback)) = self.on_text_input_callback {
            parent_div.add_default_callback_id(EventFilter::Focus(FocusEventFilter::TextInput), text_input_callback);
            parent_div.add_default_callback_id(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), vk_callback);
        }

        let label = Dom::label(field.text.clone()).with_class("__azul-native-input-text-label");
        parent_div.with_child(label)
    }
}


fn text_input_on_text_input_private<T>(info: DefaultCallbackInfoUnchecked<T>) -> CallbackReturn {
    unsafe { info.invoke_callback(text_input_on_text_input) }
}

// data: &StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>
fn text_input_on_virtual_key_down_private<T>(info: DefaultCallbackInfoUnchecked<T>) -> CallbackReturn {
    unsafe { info.invoke_callback(text_input_on_virtual_key_down) }
}

pub fn text_input_on_text_input<T>(info: DefaultCallbackInfo<T, TextInputState>) -> CallbackReturn {

    let DefaultCallbackInfo { data, state, window_id, .. } = info;
    let keyboard_state = state.windows[window_id].get_keyboard_state();

    match keyboard_state.current_char {
        Some(c) => {
            let selection = data.selection.clone();
            match selection {
                None => {
                    if data.cursor == data.text.len() {
                        data.text.push(c);
                    } else {
                        // TODO: insert character at the cursor location!
                        data.text.push(c);
                    }
                    data.cursor = data.cursor.saturating_add(1);
                },
                Some(Selection::All) => {
                    data.text = format!("{}", c);
                    data.cursor = 1;
                    data.selection = None;
                },
                Some(Selection::FromTo(range)) => {
                    delete_selection(data, range, Some(c));
                },
            }
            Redraw
        },
        None => DontRedraw,
    }
}

pub fn text_input_on_virtual_key_down<T>(info: DefaultCallbackInfo<T, TextInputState>) -> CallbackReturn {

    let DefaultCallbackInfo { data, state, window_id, .. } = info;
    let keyboard_state = state.windows[window_id].get_keyboard_state();
    let last_keycode = keyboard_state.current_virtual_keycode?;

    match last_keycode {
        VirtualKeyCode::Back => {
            // TODO: shift + back = delete last word
            let selection = data.selection.clone();
            match selection {
                None => {
                    if data.cursor == data.text.len() {
                        data.text.pop();
                    } else {
                        let mut a = data.text.chars().take(data.cursor).collect::<String>();
                        let new = data.text.len().min(data.cursor.saturating_add(1));
                        a.extend(data.text.chars().skip(new));
                        data.text = a;
                    }
                    data.cursor = data.cursor.saturating_sub(1);
                },
                Some(Selection::All) => {
                    data.text.clear();
                    data.cursor = 0;
                    data.selection = None;
                },
                Some(Selection::FromTo(range)) => {
                    delete_selection(data, range, None);
                },
            }
        },
        VirtualKeyCode::Return => {
            // TODO: selection!
            data.text.push('\n');
            data.cursor = data.cursor.saturating_add(1);
        },
        VirtualKeyCode::Home => {
            data.cursor = 0;
            data.selection = None;
        },
        VirtualKeyCode::End => {
            data.cursor = data.text.len();
            data.selection = None;
        },
        VirtualKeyCode::Escape => {
            data.selection = None;
        },
        VirtualKeyCode::Right => {
            data.cursor = data.text.len().min(data.cursor.saturating_add(1));
        },
        VirtualKeyCode::Left => {
            data.cursor = (0.max(data.cursor.saturating_sub(1))).min(data.cursor.saturating_add(1));
        },
        VirtualKeyCode::A if keyboard_state.ctrl_down => {
            data.selection = Some(Selection::All);
        },
        VirtualKeyCode::C if keyboard_state.ctrl_down => {},
        VirtualKeyCode::V if keyboard_state.ctrl_down => {},
        _ => { },
    }

    Redraw
}

fn delete_selection(data: &mut TextInputState, selection: Range<usize>, new_text: Option<char>) {
    let Range { start, end } = selection;
    let max = if end > data.text.len() { data.text.len() } else { end };

    let mut cur = start;
    if max == data.text.len() {
        data.text.truncate(start);
    } else {
        let mut a = data.text.chars().take(start).collect::<String>();

        if let Some(new) = new_text {
            a.push(new);
            cur += 1;
        }

        a.extend(data.text.chars().skip(end));
        data.text = a;
    }

    data.cursor = cur;
}