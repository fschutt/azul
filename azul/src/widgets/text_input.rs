//! Text input (demonstrates two-way data binding)

use std::ops::Range;
use {
    callbacks::{UpdateScreen, Redraw, DontRedraw},
    dom::{Dom, EventFilter, FocusEventFilter, TabIndex},
    azul_css::{ CssProperty, LayoutLeft, LayoutTop },
    text_layout::{ DEFAULT_WORD_SPACING, DEFAULT_TAB_WIDTH, DEFAULT_LINE_HEIGHT },
    window::FakeWindow,
    prelude::VirtualKeyCode,
    callbacks::{CallbackInfo, StackCheckedPointer, DefaultCallback, DefaultCallbackId},
    app::AppStateNoData,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct TextInput {
    on_text_input_callback: Option<(DefaultCallbackId, DefaultCallbackId)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextInputState {
    pub text: String,
    pub selection: Option<Selection>,
    pub cursor: usize,
    pub cursor_position: Option<(f32,f32)>
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
            cursor_position: None
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
            cursor_position: None
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
            window.add_callback(ptr, DefaultCallback(TextInputState::on_text_input_private)),
            window.add_callback(ptr, DefaultCallback(TextInputState::on_virtual_key_down_private))
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

        parent_div = parent_div.with_child(Dom::label(field.text.clone())
                               .with_class("__azul-native-input-text-label"));

        if let Some((x,y)) = field.cursor_position {
            parent_div = parent_div.with_child(Dom::div()
                                   .with_class("__azul-native-input-text-cursor")
                                   .with_css_override("left",CssProperty::Left(LayoutLeft::px(x)))
                                   .with_css_override("top",CssProperty::Top(LayoutTop::px(y))));
        }

        parent_div
    }
}

impl TextInputState {

    fn on_virtual_key_down_private<T>(data: &StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen {
        unsafe { data.invoke_mut(Self::on_virtual_key_down, app_state_no_data, window_event) }
    }

    fn on_text_input_private<T>(data: &StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen {
        unsafe { data.invoke_mut(Self::on_text_input, app_state_no_data, window_event) }
    }

    pub fn update_cursor_position<T>(&mut self, app_state_no_data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>) {
        use text_layout::WordType;

        // set to None so that it won't be displayed if the positioning fails.
        self.cursor_position = None;

        let children = event.target_children();
        if children.len() > 0 {
            let maybe_layout = &app_state_no_data.windows[event.window_id].state.last_layout_result;
            let maybe_node   = children.first(); // assuming the display label is the first child

            if let (Some(id),Some(layout)) = (maybe_node,maybe_layout) {

                let maybe_words            = layout.word_cache.get(&id);
                let maybe_positioned_words = layout.positioned_word_cache.get(&id);
                let maybe_scaled_words     = layout.scaled_words.get(&id);

                if let (Some(words),Some((scaled,_)),Some((positioned,_))) = (maybe_words,maybe_scaled_words,maybe_positioned_words) {

                    let mut scaled_iter = scaled.items.iter();

                    let space_size_px   = scaled.space_advance_px;
                    let font_size_px    = scaled.font_size_px;

                    let word_spacing_px = space_size_px * positioned.text_layout_options.word_spacing.unwrap_or(DEFAULT_WORD_SPACING);
                    let line_height_px  = space_size_px * positioned.text_layout_options.line_height.unwrap_or(DEFAULT_LINE_HEIGHT);
                    let tab_spacing_px  = word_spacing_px + space_size_px * positioned.text_layout_options.tab_width.unwrap_or(DEFAULT_TAB_WIDTH);

                    let mut x = 0.0;
                    let mut y = 0.0;

                    let line_breaks = &positioned.line_breaks;
                    let mut glyphs = vec![];

                    for w in words.items.iter() {
                        match w.word_type {
                            WordType::Word => {
                                if let Some(ref sw) = scaled_iter.next() {
                                    for gp in sw.glyph_positions.iter() {
                                        glyphs.push((gp.x_advance as f32)/128.0);
                                    }
                                }
                            },
                            WordType::Tab => { glyphs.push(tab_spacing_px); },
                            WordType::Space => { glyphs.push(word_spacing_px); },
                            _ => ()
                        }
                    }

                    x = glyphs.iter().take(self.cursor)
                                     .fold(0.0,|a,o| a + o);

                    for line_break in line_breaks.iter().take(line_breaks.len().saturating_sub(1)) {
                        x -= line_break.1;
                        y += line_height_px + font_size_px;
                    }

                    self.cursor_position = Some((x,y));
                }
            }
        }
    }

    pub fn on_virtual_key_down<T>(&mut self, app_state_no_data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>) -> UpdateScreen {

        let keyboard_state = app_state_no_data.windows[event.window_id].get_keyboard_state();
        self.update_cursor_position(app_state_no_data,event);

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

    pub fn on_text_input<T>(&mut self, app_state_no_data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>) -> UpdateScreen {

        let keyboard_state = app_state_no_data.windows[event.window_id].get_keyboard_state();
        self.update_cursor_position(app_state_no_data,event);

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