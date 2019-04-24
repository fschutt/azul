#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;

macro_rules! CSS_PATH {() => { concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/calculator/calculator.css")};}
macro_rules! FONT_PATH {() => { concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/fonts/KoHo-Light.ttf")};}

const FONT: &[u8] = include_bytes!(FONT_PATH!());

#[derive(Clone, Debug)]
enum Event {
    Clear,
    InvertSign,
    Percent,
    Divide,
    Multiply,
    Subtract,
    Plus,
    EqualSign,
    Dot,
    Number(u8),
}

#[derive(Default)]
struct Calculator {
    current_operator: Option<OperandStack>,
    current_operand_stack: OperandStack,
    division_by_zero: bool,
    expression: String,
    last_event: Option<Event>,
}

impl Layout for Calculator {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        fn numpad_btn(label: &'static str, class: &'static str) -> Dom<Calculator> {
            Dom::label(label)
                .with_class(class)
                .with_tab_index(TabIndex::Auto)
                .with_callback(On::MouseUp, Callback(handle_mouseclick_numpad_btn))
        }

        fn render_row(labels: &[&'static str; 4]) -> Dom<Calculator> {
            Dom::div()
                .with_class("row")
                .with_child(numpad_btn(labels[0], "numpad-button"))
                .with_child(numpad_btn(labels[1], "numpad-button"))
                .with_child(numpad_btn(labels[2], "numpad-button"))
                .with_child(numpad_btn(labels[3], "orange"))
        }

        let result = if self.division_by_zero {
            String::from("Cannot divide by zero.")
        } else {
            self.current_operand_stack.get_display()
        };

        Dom::div()
            .with_child(Dom::label(self.expression.to_string()).with_id("expression"))
            .with_child(Dom::label(result).with_id("result"))
            .with_child(
                Dom::div()
                    .with_id("numpad-container")
                    .with_child(render_row(&["C", "+/-", "%", "/"]))
                    .with_child(render_row(&["7", "8", "9", "x"]))
                    .with_child(render_row(&["4", "5", "6", "-"]))
                    .with_child(render_row(&["1", "2", "3", "+"]))
                    .with_child(
                        Dom::div()
                            .with_class("row")
                            .with_child(numpad_btn("0", "numpad-button").with_id("zero"))
                            .with_child(numpad_btn(".", "numpad-button"))
                            .with_child(numpad_btn("=", "orange")),
                    ),
            )
            .with_callback(EventFilter::Window(WindowEventFilter::TextInput), Callback(handle_text_input))
            .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), Callback(handle_virtual_key_input))
    }
}

#[derive(Debug, Clone, Default)]
struct OperandStack {
    stack: Vec<Number>,
    negative_number: bool,
}

impl From<f32> for OperandStack {
    fn from(value: f32) -> OperandStack {
        let mut result = OperandStack::default();
        for c in value.to_string().chars() {
            if c == '-' {
                result.negative_number = true;
            } else if c == '.' {
                result.stack.push(Number::Dot);
            } else {
                result.stack.push(Number::Value((c as u8 - 48) as u8))
            }
        }
        result
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Number {
    Value(u8),
    Dot,
}

impl OperandStack {
    /// Returns the displayable string, i.e for:
    /// `[3, 4, Dot, 5]` => `"34.5"`
    pub fn get_display(&self) -> String {
        let mut display_string = String::new();

        if self.negative_number {
            display_string.push('-');
        }

        if self.stack.is_empty() {
            display_string.push('0');
        } else {
            // If we get a dot at the end of the stack, i.e. "35." - store it,
            // but don't display it
            let mut first_dot_found = false;
            for num in &self.stack {
                match num {
                    Number::Value(v) => display_string.push((v + 48) as char),
                    Number::Dot => {
                        if !first_dot_found {
                            display_string.push('.');
                            first_dot_found = true;
                        }
                    }
                }
            }
        }

        display_string
    }

    /// Returns the number which you can use to calculate things with
    pub fn get_number(&self) -> f32 {
        let stack_size = self.stack.len();
        if stack_size == 0 {
            return 0.0;
        }

        // Iterate the stack until the first Dot is found
        let first_dot_position = self.stack.iter().position(|x| *x == Number::Dot).and_then(|x| Some(x - 1)).unwrap_or(stack_size - 1) as i32;

        let mut final_number = 0.0;

        for (number_position, number) in self.stack.iter().filter_map(|x| match x {
                Number::Dot => None,
                Number::Value(v) => Some(v),
            })
            .enumerate()
        {
            // i.e. the 5 in 5432.1 has a distance of 3 to the first dot (meaning 3 zeros)
            let diff_to_first_dot = first_dot_position - number_position as i32;
            final_number += (*number as f32) * 10.0_f32.powi(diff_to_first_dot);
        }

        if self.negative_number {
            final_number = -final_number;
        }
        final_number
    }
}

fn handle_mouseclick_numpad_btn(app_state: &mut AppState<Calculator>, event: &mut CallbackInfo<Calculator>) -> UpdateScreen {

    let mut row_iter = event.parent_nodes();
    row_iter.next()?;

    // Figure out which row and column was clicked...
    let clicked_col_idx = event.target_index_in_parent()?;
    let clicked_row_idx = row_iter.current_index_in_parent()?;

    // Figure out what button was clicked from the given row and column, filter bad events
    let event = match (clicked_row_idx, clicked_col_idx) {
        (0, 0) => Event::Clear,
        (0, 1) => Event::InvertSign,
        (0, 2) => Event::Percent,
        (0, 3) => Event::Divide,

        (1, 0) => Event::Number(7),
        (1, 1) => Event::Number(8),
        (1, 2) => Event::Number(9),
        (1, 3) => Event::Multiply,

        (2, 0) => Event::Number(4),
        (2, 1) => Event::Number(5),
        (2, 2) => Event::Number(6),
        (2, 3) => Event::Subtract,

        (3, 0) => Event::Number(1),
        (3, 1) => Event::Number(2),
        (3, 2) => Event::Number(3),
        (3, 3) => Event::Plus,

        (4, 0) => Event::Number(0),
        (4, 1) => Event::Dot,
        (4, 2) => Event::EqualSign,

        _ => return DontRedraw, // invalid item
    };

    println!("Got event via mouse input: {:?}", event);
    process_event(app_state, event)
}

fn handle_text_input(app_state: &mut AppState<Calculator>, event: &mut CallbackInfo<Calculator>) -> UpdateScreen {
    let current_key = app_state.windows[event.window_id].state.keyboard_state.current_char?;
    let event = match current_key {
        '0' => Event::Number(0),
        '1' => Event::Number(1),
        '2' => Event::Number(2),
        '3' => Event::Number(3),
        '4' => Event::Number(4),
        '5' => Event::Number(5),
        '6' => Event::Number(6),
        '7' => Event::Number(7),
        '8' => Event::Number(8),
        '9' => Event::Number(9),
        '*' => Event::Multiply,
        '-' => Event::Subtract,
        '+' => Event::Plus,
        '/' => Event::Divide,
        '%' => Event::Percent,
        '.' | ',' => Event::Dot,
        _ => return DontRedraw,
    };

    println!("Got event via keyboard input: {:?}", event);
    process_event(app_state, event)
}

fn handle_virtual_key_input(app_state: &mut AppState<Calculator>, event: &mut CallbackInfo<Calculator>) -> UpdateScreen {
    let current_key = app_state.windows[event.window_id].state.keyboard_state.latest_virtual_keycode?;
    let event = match current_key {
        VirtualKeyCode::Return => Event::EqualSign,
        VirtualKeyCode::Back => Event::Clear,
        _ => return DontRedraw,
    };
    process_event(app_state, event)
}

fn process_event(app_state: &mut AppState<Calculator>, event: Event) -> UpdateScreen {

    // Act on the event accordingly
    match event {
        Event::Clear => {
            app_state.data.modify(|state| {
                *state = Calculator::default();
            })?;
            Redraw
        }
        Event::InvertSign => {
            app_state.data.modify(|state| {
                if !state.division_by_zero {
                    state.current_operand_stack.negative_number = !state.current_operand_stack.negative_number;
                }
            })?;

            Redraw
        }
        Event::Percent => {
            app_state.data.modify(|state| {
                if state.division_by_zero {
                    return;
                }
                if let Some(operation) = &state.last_event.clone() {
                    if let Some(operand) = state.current_operator.clone() {
                        let num = state.current_operand_stack.get_number();
                        let op = operand.get_number();
                        let result = match operation {
                            Event::Plus | Event::Subtract => op / 100.0 * num,
                            Event::Multiply | Event::Divide => num / 100.0,
                            _ => unreachable!(),
                        };
                        state.current_operand_stack = OperandStack::from(result);
                    }
                }
            })?;
            Redraw
        }
        Event::EqualSign => {
            app_state.data.modify(|state| {
                if state.division_by_zero {
                    return;
                }
                if let Some(Event::EqualSign) = state.last_event {
                    state.expression = format!("{} =", state.current_operand_stack.get_display());
                }
                else {
                    state.expression.push_str(&format!("{} =", state.current_operand_stack.get_display()));
                    if let Some(operation) = &state.last_event.clone() {
                        if let Some(operand) = state.current_operator.clone() {
                            let num = state.current_operand_stack.get_number();
                            let op = operand.get_number();
                            match perform_operation(op, &operation, num) {
                                Some(r) => state.current_operand_stack = OperandStack::from(r),
                                None => state.division_by_zero = true,
                            }
                        }
                    }
                }
                state.current_operator = None;
                state.last_event = Some(Event::EqualSign);
            })?;
            Redraw
        }
        Event::Dot => {
            app_state.data.modify(|state| {
                if state.division_by_zero {
                    return;
                }
                if state.current_operand_stack.stack.iter().position(|x| *x == Number::Dot).is_none() {
                    if state.current_operand_stack.stack.len() == 0 {
                        state.current_operand_stack.stack.push(Number::Value(0));
                    }
                    state.current_operand_stack.stack.push(Number::Dot);
                }
            })?;
            Redraw
        }
        Event::Number(v) => {
            app_state.data.modify(|state| {
                if let Some(Event::EqualSign) = state.last_event {
                    *state = Calculator::default();
                }
                state.current_operand_stack.stack.push(Number::Value(v));
            });
            Redraw
        }
        operation => {
            app_state.data.modify(|state| {
                if state.division_by_zero {
                    return;
                }
                if let Some(Event::EqualSign) = state.last_event {
                    state.expression = String::new();
                }
                state.expression.push_str(&state.current_operand_stack.get_display());
                if let Some(Event::EqualSign) = state.last_event {
                    state.current_operator = Some(state.current_operand_stack.clone());
                }
                else if let Some(last_operation) = &state.last_event.clone() {
                    if let Some(operand) = state.current_operator.clone() {
                        let num = state.current_operand_stack.get_number();
                        let op = operand.get_number();
                        match perform_operation(op, last_operation, num) {
                            Some(r) => state.current_operator = Some(OperandStack::from(r)),
                            None => state.division_by_zero = true,
                        }
                    }
                }
                else {
                    state.current_operator = Some(state.current_operand_stack.clone());
                }
                state.current_operand_stack = OperandStack::default();
                state.expression.push_str(match operation {
                    Event::Plus => " + ",
                    Event::Subtract => " - ",
                    Event::Multiply => " x ",
                    Event::Divide => " / ",
                    _ => unreachable!(),
                });
                state.last_event = Some(operation);
            })?;

            Redraw
        }
    }
}

/// Performs an arithmetic operation. Returns None when trying to divide by zero.
fn perform_operation(left_operand: f32, operation: &Event, right_operand: f32) -> Option<f32> {
    match operation {
        Event::Multiply => Some(left_operand * right_operand),
        Event::Subtract => Some(left_operand - right_operand),
        Event::Plus => Some(left_operand + right_operand),
        Event::Divide => if right_operand == 0.0 {
                None
            }
			else {
                Some(left_operand / right_operand)
        },
        _ => unreachable!(),
    }
}

fn main() {

    let css = css::override_native(include_str!(CSS_PATH!())).unwrap();

    let mut app = App::new(Calculator::default(), AppConfig::default()).unwrap();
    let font_id = app.app_state.resources.add_css_font_id("KoHo-Light");
    app.app_state.resources.add_font(font_id, FontSource::Embedded(FONT));

    let window = app.create_window(WindowCreateOptions::default(), css.clone()).unwrap();
    let window2 = app.create_window(WindowCreateOptions::default(), css.clone()).unwrap();
    let window3 = app.create_window(WindowCreateOptions::default(), css.clone()).unwrap();
    let window4 = app.create_window(WindowCreateOptions::default(), css.clone()).unwrap();
    app.add_window(window2);
    app.add_window(window3);
    app.add_window(window4);
    app.run(window).unwrap();
}
