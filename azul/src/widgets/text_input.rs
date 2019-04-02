//! Text input (demonstrates two-way data binding)

use std::ops::Range;
use {
    traits::Layout,
    callbacks::{UpdateScreen, Redraw, DontRedraw},
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    window::FakeWindow,
    prelude::VirtualKeyCode,
    callbacks::{CallbackInfo, StackCheckedPointer, DefaultCallback, DefaultCallbackId},
    app::AppStateNoData,
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

struct TextInputCallback<'a> {
    ptr: &'a TextInputState,
}

impl TextInput {

    pub fn new() -> Self {
        TextInput { on_text_input_callback: None }
    }

    pub fn bind<T: Layout>(self, window: &mut FakeWindow<T>, field: &TextInputState, data: &T) -> Self {
        let ptr = StackCheckedPointer::new(data, field);
        let on_text_input_callback = ptr.map(|ptr|{(
            window.add_callback(ptr, DefaultCallback(TextInputState::on_text_input_private)),
            window.add_callback(ptr, DefaultCallback(TextInputState::on_virtual_key_down_private))
        )});

        Self {
            on_text_input_callback,
            .. self
        }
    }

    pub fn dom<T: Layout>(&self, field: &TextInputState) -> Dom<T> {

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

impl TextInputState {

    fn on_virtual_key_down_private<T: Layout>(data: &StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen {
        unsafe { data.invoke_mut(Self::on_virtual_key_down, app_state_no_data, window_event) }
    }

    fn on_text_input_private<T: Layout>(data: &StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen {
        unsafe { data.invoke_mut(Self::on_text_input, app_state_no_data, window_event) }
    }

    pub fn on_virtual_key_down<T: Layout>(&mut self, app_state_no_data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>) -> UpdateScreen {

        let keyboard_state = app_state_no_data.windows[event.window_id].get_keyboard_state();

        match keyboard_state.latest_virtual_keycode {
            Some(VirtualKeyCode::Back) => {
                // TODO: shift + back = delete last word
                let selection = self.selection.clone();
                match selection {
                    None => {
                        if self.cursor == self.text.len() {
                            self.text.pop();
                        } else {
                            let mut a = self.text.chars().take(self.cursor).collect::<String>();
                            let new = self.text.len().min(self.cursor.saturating_add(1));
                            a.extend(self.text.chars().skip(new));
                            self.text = a;
                        }
                        self.cursor = self.cursor.saturating_sub(1);
                    },
                    Some(Selection::All) => {
                        self.text.clear();
                        self.cursor = 0;
                        self.selection = None;
                    },
                    Some(Selection::FromTo(range)) => {
                        delete_selection(self, range, None);
                    },
                }

                Redraw
            },
            Some(VirtualKeyCode::Return) => {
                // TODO: selection!
                self.text.push('\n');
                self.cursor = self.cursor.saturating_add(1);
                /*
                match self.selection {
                    None => {  },
                }
                */
                Redraw
            },
            Some(VirtualKeyCode::Home) => {
                self.cursor = 0;
                self.selection = None;
                Redraw
            },
            Some(VirtualKeyCode::End) => {
                self.cursor = self.text.len();
                self.selection = None;
                Redraw
            },
            Some(VirtualKeyCode::A) if keyboard_state.ctrl_down => {
                self.selection = Some(Selection::All);
                Redraw
            },
            Some(VirtualKeyCode::Escape) => {
                self.selection = None;
                Redraw
            },
            Some(VirtualKeyCode::Right) => {
                self.cursor = self.text.len().min(self.cursor.saturating_add(1));
                Redraw
            },
            Some(VirtualKeyCode::Left) => {
                self.cursor = (0.max(self.cursor.saturating_sub(1))).min(self.cursor.saturating_add(1));
                Redraw
            },
            Some(VirtualKeyCode::C) => {
                // TODO: copy
                DontRedraw
            },
            Some(VirtualKeyCode::V) => {
                // TODO: paste
                DontRedraw
            },
            _ => DontRedraw,
        }
    }

    pub fn on_text_input<T: Layout>(&mut self, app_state_no_data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>) -> UpdateScreen {

        let keyboard_state = app_state_no_data.windows[event.window_id].get_keyboard_state();

        match keyboard_state.current_char {
            Some(c) => {
                let selection = self.selection.clone();
                match selection {
                    None => {
                        if self.cursor == self.text.len() {
                            self.text.push(c);
                        } else {
                            // TODO: insert character at the cursor location!
                            self.text.push(c);
                        }
                        self.cursor = self.cursor.saturating_add(1);
                    },
                    Some(Selection::All) => {
                        self.text = format!("{}", c);
                        self.cursor = 1;
                        self.selection = None;
                    },
                    Some(Selection::FromTo(range)) => {
                        delete_selection(self, range, Some(c));
                    },
                }
                Redraw
            },
            None => DontRedraw,
        }
    }
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