//! Text input (demonstrates two-way data binding)

use std::ops::Range;
use azul::{
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    window::{KeyboardState, VirtualKeyCode},
    callbacks::{RefAny, Callback, CallbackInfo, CallbackReturn},
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {
    on_text_input: Callback,
    on_virtual_key_down: Callback,
    state: RefAny, // Ref<TextInputState>
}

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            on_text_input: Callback { cb: Self::default_on_text_input },
            on_virtual_key_down: Callback { cb: Self::default_on_virtual_key_down },
            state: RefAny::new(TextInputState::default())
        }
    }
}

impl Into<Dom> for TextInput {
    fn into(self) -> Dom {
        self.dom()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInputState {
    pub text: String,
    pub selection: Option<Selection>,
    pub cursor_pos: usize,
}

impl Default for TextInputState {
    fn default() -> Self {
        TextInputState {
            text: String::new(),
            selection: None,
            cursor_pos: 0,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Selection {
    All,
    FromTo(Range<usize>),
}

impl TextInputState {

    #[inline]
    pub fn new<S: Into<String>>(input: S) -> Self {
        Self {
            text: input.into(),
            selection: None,
            cursor_pos: 0,
        }
    }

    #[inline]
    pub fn with_cursor_pos(self, cursor_pos: usize) -> Self {
        Self { cursor_pos, .. self }
    }

    #[inline]
    pub fn with_selection(self, selection: Option<Selection>) -> Self {
        Self { selection, .. self }
    }

    pub fn handle_on_text_input(&mut self, keyboard_state: &KeyboardState) -> Option<()> {
        let c = keyboard_state.current_char.into_option()?;

        match self.selection.clone() {
            None => {
                if self.cursor_pos == self.text.len() {
                    self.text.push(c);
                } else {
                    // TODO: insert character at the cursor location!
                    self.text.push(c);
                }
                self.cursor_pos = self.cursor_pos.saturating_add(1);
            },
            Some(Selection::All) => {
                self.text = format!("{}", c);
                self.cursor_pos = 1;
                self.selection = None;
            },
            Some(Selection::FromTo(range)) => {
                self.delete_selection(range, Some(c));
            },
        }

        Some(())
    }

    pub fn handle_on_virtual_key_down(&mut self, keyboard_state: &KeyboardState) -> Option<()> {
        let last_keycode = keyboard_state.current_virtual_keycode.into_option()?;

        match last_keycode {
            VirtualKeyCode::Back => {
                // TODO: shift + back = delete last word
                let selection = self.selection.clone();
                match selection {
                    None => {
                        if self.cursor_pos == self.text.len() {
                            self.text.pop();
                        } else {
                            let mut a = self.text.chars().take(self.cursor_pos).collect::<String>();
                            let new = self.text.len().min(self.cursor_pos.saturating_add(1));
                            a.extend(self.text.chars().skip(new));
                            self.text = a;
                        }
                        self.cursor_pos = self.cursor_pos.saturating_sub(1);
                    },
                    Some(Selection::All) => {
                        self.text.clear();
                        self.cursor_pos = 0;
                        self.selection = None;
                    },
                    Some(Selection::FromTo(range)) => {
                        self.delete_selection(range, None);
                    },
                }
            },
            VirtualKeyCode::Return => {
                // TODO: selection!
                self.text.push('\n');
                self.cursor_pos = self.cursor_pos.saturating_add(1);
            },
            VirtualKeyCode::Home => {
                self.cursor_pos = 0;
                self.selection = None;
            },
            VirtualKeyCode::End => {
                self.cursor_pos = self.text.len();
                self.selection = None;
            },
            VirtualKeyCode::Escape => {
                self.selection = None;
            },
            VirtualKeyCode::Right => {
                self.cursor_pos = self.text.len().min(self.cursor_pos.saturating_add(1));
            },
            VirtualKeyCode::Left => {
                self.cursor_pos = (0.max(self.cursor_pos.saturating_sub(1))).min(self.cursor_pos.saturating_add(1));
            },
            VirtualKeyCode::A if keyboard_state.ctrl_down => {
                self.selection = Some(Selection::All);
            },
            VirtualKeyCode::C if keyboard_state.ctrl_down => {},
            VirtualKeyCode::V if keyboard_state.ctrl_down => {},
            _ => { },
        }

        Some(())
    }

    pub fn delete_selection(&mut self, selection: Range<usize>, new_text: Option<char>) {
        let Range { start, end } = selection;
        let max = if end > self.text.len() { self.text.len() } else { end };

        let mut cur = start;
        if max == self.text.len() {
            self.text.truncate(start);
        } else {
            let mut a = self.text.chars().take(start).collect::<String>();

            if let Some(new) = new_text {
                a.push(new);
                cur += 1;
            }

            a.extend(self.text.chars().skip(end));
            self.text = a;
        }

        self.cursor_pos = cur;
    }
}

impl TextInput {

    pub fn new(state: TextInputState) -> Self {
        Self { state: RefAny::new(state), .. Default::default() }
    }

    pub fn with_state(self, state: TextInputState) -> Self {
        Self { state: RefAny::new(state), .. self }
    }

    pub fn on_text_input(self, callback: Callback) -> Self {
        Self { on_text_input: callback, .. self }
    }

    pub fn on_virtual_key_down(self, callback: Callback) -> Self {
        Self { on_text_input: callback, .. self }
    }

    pub fn dom(self) -> Dom {

        let label = Dom::label(self.state.borrow::<TextInputState>().as_ref().unwrap().text.clone().into())
            .with_class("__azul-native-input-text-label".into());

        Dom::div()
            .with_class("__azul-native-input-text".into())
            .with_tab_index(Some(TabIndex::Auto).into())
            .with_callback(EventFilter::Focus(FocusEventFilter::TextInput), self.state.clone(), self.on_text_input.cb)
            .with_callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), self.state, self.on_virtual_key_down.cb)
            .with_child(label)
    }

    pub fn default_on_text_input(info: CallbackInfo) -> CallbackReturn {
        fn default_on_text_input_inner(info: CallbackInfo) -> Option<()> {
            let state = info.get_state();
            let keyboard_state = info.get_keyboard_state();
            let text_input_state = state.borrow_mut::<TextInputState>()?;
            text_input_state.handle_on_text_input(&keyboard_state)
        }
        default_on_text_input_inner(info).into()
    }

    pub fn default_on_virtual_key_down(info: CallbackInfo) -> CallbackReturn {
        fn default_on_virtual_key_down_inner(info: CallbackInfo) -> Option<()> {
            let state = info.get_state();
            let keyboard_state = info.get_keyboard_state();
            let text_input_state = state.borrow_mut::<TextInputState>()?;
            text_input_state.handle_on_virtual_key_down(&keyboard_state)
        }
        default_on_virtual_key_down_inner(info).into()
    }
}
