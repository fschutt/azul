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

    let DefaultCallbackInfo { state, app_state_no_data, window_id, .. } = info;
    let keyboard_state = app_state_no_data.windows[window_id].get_keyboard_state();

    match keyboard_state.current_char {
        Some(c) => {
            let selection = state.selection.clone();
            match selection {
                None => {
                    if state.cursor == state.text.len() {
                        state.text.push(c);
                    } else {
                        // TODO: insert character at the cursor location!
                        state.text.push(c);
                    }
                    state.cursor = state.cursor.saturating_add(1);
                },
                Some(Selection::All) => {
                    state.text = format!("{}", c);
                    state.cursor = 1;
                    state.selection = None;
                },
                Some(Selection::FromTo(range)) => {
                    delete_selection(state, range, Some(c));
                },
            }
            Redraw
        },
        None => DontRedraw,
    }
}

pub fn text_input_on_virtual_key_down<T>(info: DefaultCallbackInfo<T, TextInputState>) -> CallbackReturn {

    let DefaultCallbackInfo { state, app_state_no_data, window_id, .. } = info;
    let keyboard_state = app_state_no_data.windows[window_id].get_keyboard_state();
    let last_keycode = keyboard_state.latest_virtual_keycode?;

    match last_keycode {
        VirtualKeyCode::Back => {
            // TODO: shift + back = delete last word
            let selection = state.selection.clone();
            match selection {
                None => {
                    if state.cursor == state.text.len() {
                        state.text.pop();
                    } else {
                        let mut a = state.text.chars().take(state.cursor).collect::<String>();
                        let new = state.text.len().min(state.cursor.saturating_add(1));
                        a.extend(state.text.chars().skip(new));
                        state.text = a;
                    }
                    state.cursor = state.cursor.saturating_sub(1);
                },
                Some(Selection::All) => {
                    state.text.clear();
                    state.cursor = 0;
                    state.selection = None;
                },
                Some(Selection::FromTo(range)) => {
                    delete_selection(state, range, None);
                },
            }
        },
        VirtualKeyCode::Return => {
            // TODO: selection!
            state.text.push('\n');
            state.cursor = state.cursor.saturating_add(1);
        },
        VirtualKeyCode::Home => {
            state.cursor = 0;
            state.selection = None;
        },
        VirtualKeyCode::End => {
            state.cursor = state.text.len();
            state.selection = None;
        },
        VirtualKeyCode::Escape => {
            state.selection = None;
        },
        VirtualKeyCode::Right => {
            state.cursor = state.text.len().min(state.cursor.saturating_add(1));
        },
        VirtualKeyCode::Left => {
            state.cursor = (0.max(state.cursor.saturating_sub(1))).min(state.cursor.saturating_add(1));
        },
        VirtualKeyCode::A if keyboard_state.ctrl_down => {
            state.selection = Some(Selection::All);
        },
        VirtualKeyCode::C if keyboard_state.ctrl_down => {},
        VirtualKeyCode::V if keyboard_state.ctrl_down => {},
        _ => { },
    }

    Redraw
}

fn delete_selection(state: &mut TextInputState, selection: Range<usize>, new_text: Option<char>) {
    let Range { start, end } = selection;
    let max = if end > state.text.len() { state.text.len() } else { end };

    let mut cur = start;
    if max == state.text.len() {
        state.text.truncate(start);
    } else {
        let mut a = state.text.chars().take(start).collect::<String>();

        if let Some(new) = new_text {
            a.push(new);
            cur += 1;
        }

        a.extend(state.text.chars().skip(end));
        state.text = a;
    }

    state.cursor = cur;
}