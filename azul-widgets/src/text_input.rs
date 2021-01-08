//! Text input (demonstrates two-way data binding)

use std::ops::Range;
use azul::{
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    window::{KeyboardState, VirtualKeyCode},
    callbacks::{RefAny, Callback, CallbackInfo, CallbackReturn, UpdateScreen},
};

pub type OnTextInputFn = fn(&mut RefAny, &mut TextInputState, CallbackInfo) -> UpdateScreen;
pub type OnVirtualKeyDownFn = fn(&mut RefAny, &mut TextInputState, CallbackInfo) -> UpdateScreen;

#[derive(Debug, Clone)]
pub enum VirtualKeyAction {
    CopyToClipboard,
    PasteFromClipboard,
}

#[derive(Debug, Clone)]
pub struct TextInput {
    state: TextInputState,
    style: Css,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TextInputSelection {
    All,
    FromTo(Range<usize>),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TextInputState {
    pub text: String,
    pub on_text_input: Option<(OnTextInputFn, RefAny)>,
    pub on_virtual_key_down: Option<(OnVirtualKeyDownFn, RefAny)>,
    pub update_text_input_before_calling_text_input_fn: bool,
    pub update_text_input_before_calling_vk_down_fn: bool,
    pub selection: Option<TextInputSelection>,
    pub cursor_pos: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            state: TextInputState::default(),
            style: TextInput::native_style(),
        }
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        TextInputState {
            text: String::new(),
            on_text_input: None,
            on_virtual_key_down: None,
            update_text_input_before_calling_text_input_fn: true,
            update_text_input_before_calling_vk_down_fn: true,
            selection: None,
            cursor_pos: 0,
        }
    }
}

impl TextInput {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(self, state: TextInputState) -> Self {
        Self { state, .. self }
    }

    pub fn override_on_text_input(self, callback: OnTextInputFn, on_text_input_data: RefAny) -> Self {
        Self { on_text_input: callback, .. self }
    }

    pub fn override_on_virtual_key_down(self, callback: OnVirtualKeyDownFn, on_virtual_key_down_data: RefAny) -> Self {
        Self { on_text_input: callback, .. self }
    }


    pub fn with_style(self, css: Css) -> Self {
        Self { style: css, .. self }
    }

    /// Returns the native style for the button, differs based on operating system
    pub fn native_css() -> Css {
        #[cfg(target_os = "windows")] { Self::windows_css() }
        #[cfg(target_os = "mac")] { Self::mac_css() }
        #[cfg(target_os = "linux")] { Self::linux_css() }
        #[cfg(not(any(target_os = "windows", target_os = "mac", target_os = "linux")))] { Self::web_css() }
    }

    pub fn windows_css() -> Css {
        Css::from_string("
            .__azul-native-input-text {
                display: flex;
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
            }

            .__azul-native-input-text:hover {
                border: 1px solid #4286f4;
            }".to_string().into()
        )
    }

    pub fn linux_css() -> Css {
        Css::from_string("
           .__azul-native-input-text {
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
           }

           .__azul-native-input-text:hover {
               border: 1px solid #4286f4;
           }

           .__azul-native-input-text-label {
               font-size: 16px;
               font-family: sans-serif;
               color: #4c4c4c;
               display: flex;
               flex-grow: 1;
           }".to_string().into()
        )
    }

    pub fn mac_css() -> Css {
        Css::from_string("
            .__azul-native-input-text {
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
            }

            .__azul-native-input-text:hover {
                border: 1px solid #4286f4;
            }

            .__azul-native-input-text-label {
                font-size: 12px;
                font-family: \"Helvetica\";
                color: #4c4c4c;
            }".to_string().into()
        )
    }

    pub fn web_css() -> Css {
        Css::empty() // TODO
    }

    pub fn dom(self) -> StyledDom {

        let label = Dom::label(self.state.text.as_ref().into())
            .with_class("__azul-native-input-text-label".into());

        // let text_selection = Dom::div().with_class("__azul-native-input-text-selection".into());

        let state_ref = RefAny::new(self.state);

        let container = Dom::div()
            .with_class("__azul-native-input-text".into())
            .with_tab_index(Some(TabIndex::Auto).into())
            .with_dataset(state_ref.clone())
            .with_callback(EventFilter::Focus(FocusEventFilter::TextInput), state_ref.clone(), Self::default_on_text_input)
            .with_callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), state_ref, Self::default_on_virtual_key_down)
            .with_child(label);
            // .with_child(text_selection);

        StyledDom::new(container, self.style)
    }

    extern "C" fn default_on_text_input(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {

        fn default_on_text_input_inner(text_input: &mut RefAny, info: CallbackInfo) -> Option<()> {

            let text_input = text_input.downcast_mut::<TextInputState>()?;
            let keyboard_state = info.get_keyboard_state();
            let c = keyboard_state.current_char.into_option()?;
            let c = std::char::from_u32(c)?;

            if text_input_state.update_text_input_before_calling_vk_down_fn {
                text_input.handle_on_text_input(c);
            }

            // let label = info.get_first_child(info.get_hit_node()?)?;
            // let text_selection = info.get_next_sibling(label)?;
            // info.set_text(label, text_input.text.as_ref().into()); // update text on screen
            // info.start_timer(scroll_to_left(label)); // start timer to update scroll position of label
            // info.update_image(label, text_selection.selection.to_image_mask(info.get_gl_context()))?; // update selection

            let (on_text_input_fn, on_text_input_data) = text_input.on_text_input.as_mut()?;
            (on_text_input_fn)(on_text_input_data, text_input, info)

            None
        }

        let _ = default_on_text_input_inner(text_input, info);

        UpdateScreen::DoNothing
    }

    extern "C" fn default_on_virtual_key_down(text_input: &mut RefAny, info: CallbackInfo) -> UpdateScreen {

        fn default_on_virtual_key_down_inner(text_input_state: &mut RefAny, info: CallbackInfo) -> Option<()> {

            let text_input_state = text_input_state.downcast_mut::<TextInputState>()?;
            let keyboard_state = info.get_keyboard_state();
            let last_keycode = keyboard_state.current_virtual_keycode.into_option()?;

            if text_input_state.update_text_input_before_calling_vk_down_fn {
                match text_input_state.handle_on_virtual_key_down(last_keycode)? {
                    VirtualKeyAction::CopyToClipboard => {
                        // info.set_clipboard_contents(&text_input_state.text);
                    },
                    VirtualKeyAction::PasteFromClipboard => {
                        // text_input_state.text = info.get_clipboard_contents();
                    }
                }
            }

            let (on_virtual_key_down_fn, on_virtual_key_down_data) = text_input.on_virtual_key_down.as_mut()?;
            (on_virtual_key_down_fn)(on_virtual_key_down_data, text_input, info)
        }

        let _ = default_on_virtual_key_down_inner(text_input, info);

        UpdateScreen::DoNothing
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
                    self.text.push(c);
                }
                self.cursor_pos = self.cursor_pos.saturating_add(1);
            },
            Some(TextInputSelection::All) => {
                self.text = format!("{}", c);
                self.cursor_pos = 1;
                self.selection = None;
            },
            Some(TextInputSelection::FromTo(range)) => {
                self.delete_selection(range, Some(c));
            },
        }
    }

    fn handle_on_virtual_key_down(&mut self, virtual_key: VirtualKeyCoder) -> Option<VirtualKeyAction> {
        match virtual_key {
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
                    Some(TextInputSelection::All) => {
                        self.text.clear();
                        self.cursor_pos = 0;
                        self.selection = None;
                    },
                    Some(TextInputSelection::FromTo(range)) => {
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
                self.selection = Some(TextInputSelection::All);
            },
            VirtualKeyCode::C if keyboard_state.ctrl_down => {
                return Some(VirtualKeyAction::CopyToClipboard);
            },
            VirtualKeyCode::V if keyboard_state.ctrl_down => {
                return Some(VirtualKeyAction::PasteFromClipboard);
            },
            _ => { },
        }

        None
    }

    fn delete_selection(&mut self, selection: Range<usize>, new_text: Option<char>) {
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

impl From<TextInput> for StyledDom {
    fn into(self) -> StyledDom {
        self.dom()
    }
}