extern crate azul;

use azul::prelude::*;

const FONT: &[u8] = include_bytes!("../assets/fonts/KoHo-Light.ttf");

#[derive(Default)]
struct Calculator {
    current_operator: Option<OperandStack>,
    result: Option<f32>,
    current_operand_stack: OperandStack,
}

impl Layout for Calculator {
    fn layout(&self, _info: WindowInfo<Self>) -> Dom<Self> {

        fn numpad_btn(label: &str, class: &str) -> Dom<Calculator> {
            Dom::label(label).with_class(class).with_callback(On::MouseUp, Callback(handle_mouseclick_numpad_btn))
        }

        fn render_row(labels: &[&str;4]) -> Dom<Calculator> {
            Dom::div().with_class("row")
                .with_child(numpad_btn(labels[0], "numpad-button"))
                .with_child(numpad_btn(labels[1], "numpad-button"))
                .with_child(numpad_btn(labels[2], "numpad-button"))
                .with_child(numpad_btn(labels[3], "orange"))
        }

        let result = self.result.and_then(|r| Some(r.to_string())).unwrap_or(self.current_operand_stack.get_display());

        Dom::div()
            .with_child(Dom::label(result).with_id("result"))
            .with_child(Dom::div().with_id("numpad-container")
                .with_child(render_row(&["C", "+/-", "%", "/"]))
                .with_child(render_row(&["7", "8", "9", "x"]))
                .with_child(render_row(&["4", "5", "6", "-"]))
                .with_child(render_row(&["1", "2", "3", "+"]))
                .with_child(Dom::div().with_class("row")
                    .with_child(numpad_btn("0", "numpad-button").with_id("zero"))
                    .with_child(numpad_btn(".", "numpad-button"))
                    .with_child(numpad_btn("=", "orange"))
                )
            )
    }
}

#[derive(Debug, Clone, Default)]
struct OperandStack {
    stack: Vec<Number>,
    negative_number: bool,
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
                    Number::Dot => if !first_dot_found {
                        display_string.push('.');
                        first_dot_found = true;
                    },
                }
            }
        }

        display_string
    }

    /// Returns the number which you can use to calculate things with
    pub fn get_number(&self) -> f32 {

        // Iterate the stack until the first Dot is found
        let stack_size = self.stack.len();
        let first_dot_position = self.stack.iter().position(|x| *x == Number::Dot).unwrap_or(stack_size - 1) as i32;

        let mut final_number = 0.0;

        for (number_position, number) in self.stack.iter().filter_map(|x| match x {
            Number::Dot => None,
            Number::Value(v) => Some(v),
        }).enumerate() {
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

fn handle_mouseclick_numpad_btn(app_state: &mut AppState<Calculator>, event: WindowEvent<Calculator>) -> UpdateScreen {

    let mut row_iter = event.index_path_iter();

    // Figure out which row and column was clicked...
    let clicked_col_idx = match row_iter.next() {
        Some(s) => s,
        None => return UpdateScreen::DontRedraw,
    };

    let clicked_row_idx = match row_iter.next() {
        Some(s) => s,
        None => return UpdateScreen::DontRedraw,
    };

    #[derive(Debug)]
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

        _ => return UpdateScreen::DontRedraw, // invalid item
    };

    println!("got event: {:?}", event);

    // Act on the event accordingly
    match event {
        Event::Clear => {
            app_state.data.modify(|state| {
                state.current_operator = None;
                state.result = None;
                state.current_operand_stack = OperandStack::default();
            });
            UpdateScreen::Redraw
        },
        Event::InvertSign => {
            app_state.data.modify(|state| {
                state.current_operand_stack.negative_number = !state.current_operand_stack.negative_number;
            });

            UpdateScreen::Redraw
        },
        Event::Percent => {
            // TODO: not sure what this button does...
            UpdateScreen::DontRedraw
        },
        Event::EqualSign => {
            app_state.data.modify(|state| {
                if state.current_operator.is_none() {
                    state.current_operator = Some(state.current_operand_stack.clone());
                    state.current_operand_stack = OperandStack::default();
                }
            });
            UpdateScreen::Redraw
        },
        Event::Dot => {
            app_state.data.modify(|state| {
                state.current_operand_stack.stack.push(Number::Dot);
            });
            UpdateScreen::Redraw
        },
        Event::Number(v) => {
            app_state.data.modify(|state| {
                state.current_operand_stack.stack.push(Number::Value(v));
            });
            UpdateScreen::Redraw
        },
        operation => {
            app_state.data.modify(|state| {
                if let Some(operand) = &state.current_operator {
                    let num = state.current_operand_stack.get_number();
                    let op = operand.get_number();
                    let result = match operation {
                        Event::Multiply => op * num,
                        Event::Subtract => op - num,
                        Event::Plus => op + num,
                        Event::Divide => op / num,
                        _ => return,
                    };
                    state.result = Some(result);
                }
            });

            UpdateScreen::Redraw
        },
    }
}

fn main() {
    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/calculator.css")) }

    let mut app = App::new(Calculator::default(), AppConfig::default());
    app.add_font(FontId::ExternalFont("KoHo-Light".into()), &mut FONT.clone()).unwrap();
    let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
    let window = Window::new(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}
