//! Text input (demonstrates two-way data binding)

use core::ops::Range;
use azul::{
    css::Css,
    str::String as AzString,
    style::StyledDom,
    vec::NodeDataInlineCssPropertyVec,
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    window::{KeyboardState, VirtualKeyCode},
    callbacks::{RefAny, Callback, CallbackInfo, UpdateScreen},
};

#[derive(Debug, Clone)]
pub struct TextInput {
    pub state: TextInputStateWrapper,
    pub placeholder_style: NodeDataInlineCssPropertyVec,
    pub placeholder_hover_style: NodeDataInlineCssPropertyVec,
    pub placeholder_focus_style: NodeDataInlineCssPropertyVec,
    pub container_style: NodeDataInlineCssPropertyVec,
    pub container_hover_style: NodeDataInlineCssPropertyVec,
    pub container_focus_style: NodeDataInlineCssPropertyVec,
    pub label_style: NodeDataInlineCssPropertyVec,
    pub label_hover_style: NodeDataInlineCssPropertyVec,
    pub label_focus_style: NodeDataInlineCssPropertyVec,
}

pub struct CustomCallbackFn {
    pub cb: extern "C" fn(&mut RefAny, &TextInputState, CallbackInfo) -> UpdateScreen,
}

impl_callback!(CustomCallbackFn);

#[derive(Debug, Clone, PartialEq)]
pub struct TextInputState {
    pub text: Vec<char>,
    pub placeholder: Option<AzString>,
    pub max_len: usize,
    pub selection: Option<TextInputSelection>,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextInputStateWrapper {
    pub inner: TextInputState,
    pub on_text_input: Option<(CustomCallbackFn, RefAny)>,
    pub on_virtual_key_down: Option<(CustomCallbackFn, RefAny)>,
    pub on_focus_lost: Option<(CustomCallbackFn, RefAny)>,
    pub update_text_input_before_calling_text_input_fn: bool,
    pub update_text_input_before_calling_focus_lost_fn: bool,
    pub update_text_input_before_calling_vk_down_fn: bool,
}

static TEXT_INPUT_CONTAINER_WINDOWS: &[NodeDataInlineCssProperty] = &[
    /*
        box-sizing: border-box;
        font-size: 13px;
        flex-grow: 1;
        background-color: white;
        border: 1px solid #9b9b9b;
        padding: 1px;
        overflow: hidden;
        text-align: left;
        flex-direction: row;
        align-content: flex-end;
        justify-content: flex-end;
        font-family: sans-serif;
    */
];

static TEXT_INPUT_CONTAINER_LINUX = [
    /*
        font-size: 16px;
        font-family: sans-serif;
        color: #4c4c4c;
        display: flex;
        flex-grow: 1;
        background-color: white;
        border: 1px solid #9b9b9b;
        padding: 1px;
        overflow: hidden;
        text-align: left;
        flex-direction: row;
        align-content: flex-end;
        justify-content: flex-end;
        box-sizing: border-box;
    */
];

static TEXT_INPUT_CONTAINER_MAC = [
    /*
        font-size: 12px;
        font-family: \"Helvetica\";
        color: #4c4c4c;
        background-color: white;
        height: 14px;
        border: 1px solid #9b9b9b;
        padding: 1px;
        overflow: hidden;
        text-align: left;
        flex-direction: row;
        align-content: flex-end;
        justify-content: flex-end;
    */
];

static TEXT_INPUT_CONTAINER_HOVER = [
    /*
        border: 1px solid #4286f4;
    */
];

static TEXT_INPUT_LABEL_WINDOWS = [
    /*
        font-family: sans-serif;
    */
];

static TEXT_INPUT_LABEL_LINUX = [
    /*
        font-size: 16px;
        font-family: sans-serif;
        color: #4c4c4c;
        display: flex;
        flex-grow: 1;
    */
];

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            state: TextInputStateWrapper::default(),
            placeholder_style: Vec::new().into(),
            placeholder_hover_style: Vec::new().into(),
            placeholder_focus_style: Vec::new().into(),
            container_style: Vec::new().into(),
            container_hover_style: Vec::new().into(),
            container_focus_style: Vec::new().into(),
            /*
                LINUX:
                .__azul-native-input-text-label {
                    font-size: 16px;
                    font-family: sans-serif;
                    color: #4c4c4c;
                    display: flex;
                    flex-grow: 1;
                }

                MAC:
                .__azul-native-input-text-label {
                    font-size: 12px;
                    font-family: \"Helvetica\";
                    color: #4c4c4c;
                }
            */
            label_style: Vec::new().into(),
            label_hover_style: Vec::new().into(),
            label_focus_style: Vec::new().into(),
        }
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        TextInputState {
            text: Vec::new(),
            placeholder: None,
            max_len: 50,
            selection: None,
            cursor_pos: 0,
        }
    }
}

impl Default for TextInputStateWrapper {
    fn default() -> Self {
        TextInputStateWrapper {
            inner: TextInputState::default(),
            on_text_input: None,
            on_virtual_key_down: None,
            on_focus_lost: None,
            update_text_input_before_calling_text_input_fn: true,
            update_text_input_before_calling_focus_lost_fn: true,
            update_text_input_before_calling_vk_down_fn: true,
        }
    }
}

impl TextInput {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_text_input(mut self, callback: CustomCallbackFn, data: RefAny) -> Self {
        self.state.on_text_input = Some((callback, data));
        self
    }

    pub fn on_virtual_key_down(self, callback: CustomCallbackFn, data: RefAny) -> Self {
        self.state.on_virtual_key_down = Some((callback, data));
        self
    }

    pub fn on_focus_lost(self, callback: CustomCallbackFn, data: RefAny) -> Self {
        self.state.on_focus_lost = Some((callback, data));
        self
    }

    pub fn dom(self) -> StyledDom {

        use azul::dom::{CallbackData, EventFilter, HoverEventFilter, FocusEventFilter};

        let state_ref = RefAny::new(self.state);

        Dom::div()
        .with_class("__azul-native-text-input-text".into())
        .with_tab_index(Some(TabIndex::Auto).into())
        .with_dataset(state_ref.clone())
        .with_callbacks(vec![
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                data: state_ref.clone(),
                callback: Callback { cb: self::input::default_on_focus_received }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusLost),
                data: state_ref.clone(),
                callback: Callback { cb: self::input::default_on_focus_lost }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::TextInput),
                data: state_ref.clone(),
                callback: Callback { cb: self::input::default_on_text_input }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                data: state_ref.clone(),
                callback: Callback { cb: self::input::default_on_virtual_key_down }
            },
            CallbackData {
                event: EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                data: state_ref.clone(),
                callback: Callback { cb: self::input::default_on_container_click }
            },
        ].into())
        .with_children(vec![
            Dom::text(self.state.inner.text.iter().collect::<String>().into())
            .with_class("__azul-native-input-text-label".into()),
            // let cursor = Dom::div().with_class("__azul-native-text-input-cursor");
            // let text_selection = Dom::div().with_class("__azul-native-text-input-selection".into());
        ].into()).style(Css::empty())
    }
}

// handle input events for the TextInput
mod input {

    use azul::callbacks::{RefAny, CallbackInfo, UpdateScreen};
    use super::TextInputStateWrapper;

    pub(in super) extern "C" fn default_on_text_input(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {

        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        let keyboard_state = info.get_keyboard_state();

        let c = match keyboard_state.current_char.into_option() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        let c = match core::char::from_u32(c) {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        let label_node_id = match info.get_first_child(info.get_hit_node()).into_option() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        if text_input.update_text_input_before_calling_vk_down_fn {
            text_input.inner.handle_on_text_input(c);
        }

        let result = match text_input.on_text_input.as_mut() {
            Some((f, d)) => (f.cb)(d, &text_input.inner, info),
            None => UpdateScreen::DoNothing,
        };

        if !text_input.update_text_input_before_calling_vk_down_fn {
            text_input.inner.handle_on_text_input(c);
        }

        // Update the string, cursor position on the screen and selection background
        // TODO: restart the timer for cursor blinking
        info.set_string_contents(label_node_id, text_input.inner.text.iter().collect::<String>().into());
        // info.set_css_property(cursor_node_id, CssProperty::transform(get_cursor_transform(info.get_text_contents()[self.cursor_pos])))
        // info.update_image(selection_node_id, render_selection(self.selection));

        result
    }

    pub(in super) extern "C" fn default_on_virtual_key_down(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {

        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };
        let keyboard_state = info.get_keyboard_state();
        let last_keycode = match keyboard_state.current_virtual_keycode.into_option() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        let kb_state = info.get_keyboard_state();

        if text_input.update_text_input_before_calling_vk_down_fn {
            let _ = text_input.inner.handle_on_virtual_key_down(last_keycode, &kb_state, info);
        }

        let result = match text_input.on_virtual_key_down.as_mut() {
            Some((f, d)) => (f.cb)(d, &text_input.inner, info),
            None => UpdateScreen::DoNothing,
        };

        if !text_input.update_text_input_before_calling_vk_down_fn {
            let _ = text_input.inner.handle_on_virtual_key_down(last_keycode, &kb_state, info);
        }

        result
    }

    pub(in super) extern "C" fn default_on_container_click(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {
        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };
        // TODO: clear selection, set cursor to text hit
        UpdateScreen::DoNothing
    }

    pub(in super) extern "C" fn default_on_label_click(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {
        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };
        // TODO: set cursor to end or start
        UpdateScreen::DoNothing
    }

    pub(in super) extern "C" fn default_on_focus_received(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {
        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        // TODO: start text cursor blinking
        UpdateScreen::DoNothing
    }

    pub(in super) extern "C" fn default_on_focus_lost(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {
        let text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
            Some(s) => s,
            None => return UpdateScreen::DoNothing,
        };

        let result = match text_input.on_focus_lost.as_mut() {
            Some((f, d)) => (f.cb)(d, &text_input.inner, info),
            None => UpdateScreen::DoNothing,
        };

        result
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TextInputSelection {
    All,
    FromTo(Range<usize>),
}

impl TextInputSelection {
    pub fn get_range(&self, text_len: usize) -> Range<usize> {
        match self {
            TextInputSelection::All => 0..text_len,
            TextInputSelection::FromTo(r) => *r,
        }
    }
}

impl TextInputState {

    fn handle_on_text_input(&mut self, c: char) {
        match self.selection.clone() {
            None => {
                if self.cursor_pos == self.text.len() {
                    self.text.push(c);
                } else {
                    // TODO: insert character at the cursor location!
                    self.text.insert(self.cursor_pos, c);
                }
                self.cursor_pos = self.cursor_pos.saturating_add(1).min(self.text.len() - 1);
            },
            Some(TextInputSelection::All) => {
                self.text = vec![c];
                self.cursor_pos = 1;
                self.selection = None;
            },
            Some(TextInputSelection::FromTo(range)) => {
                self.delete_selection(range, Some(c));
            },
        }
    }

    fn handle_on_virtual_key_down(
        &mut self,
        virtual_key: VirtualKeyCode,
        keyboard_state: &KeyboardState,
        info: CallbackInfo,
    ) {
        match virtual_key {
            VirtualKeyCode::Back => {
                // TODO: shift + back = delete last word
                let selection = self.selection.clone();
                match selection {
                    None => {
                        if self.cursor_pos == (self.text.len() - 1) {
                            self.text.pop();
                        } else {
                            self.text.remove(self.cursor_pos);
                        }
                        self.cursor_pos = self.cursor_pos.saturating_sub(1);
                    },
                    Some(TextInputSelection::All) => {
                        self.text = Vec::new();
                        self.cursor_pos = 0;
                        self.selection = None;
                    },
                    Some(TextInputSelection::FromTo(range)) => {
                        self.delete_selection(range, None);
                    },
                }
            },
            VirtualKeyCode::Return => { /* ignore return keys */ },
            VirtualKeyCode::Home => {
                self.cursor_pos = 0;
                self.selection = None;
            },
            VirtualKeyCode::End => {
                self.cursor_pos = self.text.len().saturating_sub(1);
                self.selection = None;
            },
            VirtualKeyCode::Tab => {
                use azul::callbacks::FocusTarget;
                if keyboard_state.shift_down {
                    info.set_focus(FocusTarget::Next);
                } else {
                    info.set_focus(FocusTarget::Previous);
                }
            },
            VirtualKeyCode::Escape => {
                self.selection = None;
            },
            VirtualKeyCode::Right => {
                self.cursor_pos = self.cursor_pos.saturating_add(1).min(self.text.len());
            },
            VirtualKeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1).min(self.text.len());
            },
            // ctrl + a
            VirtualKeyCode::A if keyboard_state.ctrl_down => {
                self.selection = Some(TextInputSelection::All);
            },
            /*
            // ctrl + c
            VirtualKeyCode::C if keyboard_state.ctrl_down => {
                Clipboard::new().set_string_contents(self.text[self.selection]);
            },
            // ctrl + v
            VirtualKeyCode::V if keyboard_state.ctrl_down => {
                let clipboard_contents = Clipboard::new().get_string_contents(self.text[self.selection]).into_option().unwrap_or_default();
                let clipboard_contents: Vec<char> = clipboard_contents.as_str().chars().collect();
                match self.selection {
                    None => {

                    },
                    Some(TextInputSelection::All) => {
                        self.text = ;
                    },
                    Some(TextInputSelection::Range(r)) => {
                        self.delete_selection(r);


                    }
                }
                self.selection = None;
                self.cursor = ...;
            },
            // ctrl + x
            VirtualKeyCode::X if keyboard_state.ctrl_down => {
                Clipboard::new().set_string_contents(self.text[self.selection.get_range(self.text.len())]);
            }
            */
            _ => { },
        }
    }

    fn delete_selection(&mut self, selection: Range<usize>, new_text: Option<char>) {
        let Range { start, end } = selection;
        let max = end.min(self.text.len() - 1);

        let mut cur = start;

        if max == (self.text.len() - 1) {
            self.text.truncate(start);
        } else {
            let end = &self.text[end.min(self.text.len() - 1)..].to_vec();
            self.text.truncate(start);
            self.text.extend(end.iter());
        }

        self.cursor_pos = start;
    }
}

impl From<TextInput> for StyledDom {
    fn from(t: TextInput) -> StyledDom {
        t.dom()
    }
}