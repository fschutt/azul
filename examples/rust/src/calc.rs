//! Calculator example demonstrating CSS Grid layout and composability
//! 
//! This example showcases how to build a calculator UI using CSS Grid,
//! demonstrating Azul's layout capabilities and component composition.

#![windows_subsystem = "windows"]

use azul::{
    app::{App, AppConfig, LayoutSolver},
    callbacks::{CallbackInfo, LayoutCallbackInfo, RefAny, Update},
    css::Css,
    dom::Dom,
    style::StyledDom,
    window::WindowCreateOptions,
};

/// Calculator state
struct Calculator {
    display: String,
    current_value: f64,
    pending_operation: Option<Operation>,
    pending_value: Option<f64>,
    clear_on_next_input: bool,
}

#[derive(Clone, Copy)]
enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Calculator {
    fn new() -> Self {
        Self {
            display: "0".to_string(),
            current_value: 0.0,
            pending_operation: None,
            pending_value: None,
            clear_on_next_input: false,
        }
    }

    fn input_digit(&mut self, digit: char) {
        if self.clear_on_next_input {
            self.display = String::new();
            self.clear_on_next_input = false;
        }
        if self.display == "0" && digit != '.' {
            self.display = digit.to_string();
        } else if digit == '.' && self.display.contains('.') {
            // Don't add another decimal point
        } else {
            self.display.push(digit);
        }
        self.current_value = self.display.parse().unwrap_or(0.0);
    }

    fn set_operation(&mut self, op: Operation) {
        self.calculate();
        self.pending_operation = Some(op);
        self.pending_value = Some(self.current_value);
        self.clear_on_next_input = true;
    }

    fn calculate(&mut self) {
        if let (Some(op), Some(pending)) = (self.pending_operation, self.pending_value) {
            let result = match op {
                Operation::Add => pending + self.current_value,
                Operation::Subtract => pending - self.current_value,
                Operation::Multiply => pending * self.current_value,
                Operation::Divide => {
                    if self.current_value != 0.0 {
                        pending / self.current_value
                    } else {
                        f64::NAN
                    }
                }
            };
            self.current_value = result;
            self.display = if result.is_nan() {
                "Error".to_string()
            } else if result.fract() == 0.0 && result.abs() < 1e15 {
                format!("{}", result as i64)
            } else {
                format!("{}", result)
            };
            self.pending_operation = None;
            self.pending_value = None;
            self.clear_on_next_input = true;
        }
    }

    fn clear(&mut self) {
        self.display = "0".to_string();
        self.current_value = 0.0;
        self.pending_operation = None;
        self.pending_value = None;
        self.clear_on_next_input = false;
    }

    fn invert_sign(&mut self) {
        self.current_value = -self.current_value;
        self.display = if self.current_value.fract() == 0.0 {
            format!("{}", self.current_value as i64)
        } else {
            format!("{}", self.current_value)
        };
    }

    fn percent(&mut self) {
        self.current_value /= 100.0;
        self.display = format!("{}", self.current_value);
    }
}

// Event type for button callbacks
#[derive(Clone)]
enum CalcEvent {
    Digit(char),
    Operation(Operation),
    Equals,
    Clear,
    InvertSign,
    Percent,
}

struct ButtonData {
    calc: RefAny,
    event: CalcEvent,
}

extern "C" fn on_button_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let button_data = match data.downcast_ref::<ButtonData>() {
        Some(d) => d,
        None => return Update::DoNothing,
    };

    let mut calc = match button_data.calc.downcast_mut::<Calculator>() {
        Some(c) => c,
        None => return Update::DoNothing,
    };

    match &button_data.event {
        CalcEvent::Digit(d) => calc.input_digit(*d),
        CalcEvent::Operation(op) => calc.set_operation(*op),
        CalcEvent::Equals => calc.calculate(),
        CalcEvent::Clear => calc.clear(),
        CalcEvent::InvertSign => calc.invert_sign(),
        CalcEvent::Percent => calc.percent(),
    }

    Update::RefreshDom
}

/// Creates a calculator button with the given label and event
fn calc_button(calc: &RefAny, label: &str, event: CalcEvent, class: &str) -> Dom {
    let button_data = RefAny::new(ButtonData {
        calc: calc.clone(),
        event,
    });

    Dom::div()
        .with_inline_style(class)
        .with_child(Dom::text(label))
        .with_dataset(Some(button_data))
        .with_callback(azul::callbacks::On::MouseUp, on_button_click)
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    let display_text = match data.downcast_ref::<Calculator>() {
        Some(calc) => calc.display.clone(),
        None => "0".to_string(),
    };

    // CSS styles
    let calculator_style = "
        height: 100%;
        display: flex;
        flex-direction: column;
        font-family: sans-serif;
    ";

    let display_style = "
        background-color: #2d2d2d;
        color: white;
        font-size: 48px;
        text-align: right;
        padding: 20px;
        font-weight: 300;
        display: flex;
        align-items: center;
        justify-content: flex-end;
        min-height: 80px;
    ";

    let buttons_style = "
        flex-grow: 1;
        display: grid;
        grid-template-columns: 1fr 1fr 1fr 1fr;
        grid-template-rows: 1fr 1fr 1fr 1fr 1fr;
        gap: 1px;
        background-color: #666666;
    ";

    let button_style = "
        background-color: #d1d1d6;
        color: #1d1d1f;
        font-size: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
    ";

    let operator_style = "
        background-color: #ff9f0a;
        color: white;
        font-size: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
    ";

    let zero_style = "
        background-color: #d1d1d6;
        color: #1d1d1f;
        font-size: 24px;
        display: flex;
        align-items: center;
        justify-content: flex-start;
        padding-left: 28px;
        grid-column: span 2;
        cursor: pointer;
    ";

    // Build the calculator DOM using CSS Grid
    let display = Dom::div()
        .with_inline_style(display_style)
        .with_child(Dom::text(display_text));

    let buttons = Dom::div()
        .with_inline_style(buttons_style)
        // Row 1: C, +/-, %, ÷
        .with_child(calc_button(data, "C", CalcEvent::Clear, button_style))
        .with_child(calc_button(data, "+/-", CalcEvent::InvertSign, button_style))
        .with_child(calc_button(data, "%", CalcEvent::Percent, button_style))
        .with_child(calc_button(data, "÷", CalcEvent::Operation(Operation::Divide), operator_style))
        // Row 2: 7, 8, 9, ×
        .with_child(calc_button(data, "7", CalcEvent::Digit('7'), button_style))
        .with_child(calc_button(data, "8", CalcEvent::Digit('8'), button_style))
        .with_child(calc_button(data, "9", CalcEvent::Digit('9'), button_style))
        .with_child(calc_button(data, "×", CalcEvent::Operation(Operation::Multiply), operator_style))
        // Row 3: 4, 5, 6, -
        .with_child(calc_button(data, "4", CalcEvent::Digit('4'), button_style))
        .with_child(calc_button(data, "5", CalcEvent::Digit('5'), button_style))
        .with_child(calc_button(data, "6", CalcEvent::Digit('6'), button_style))
        .with_child(calc_button(data, "-", CalcEvent::Operation(Operation::Subtract), operator_style))
        // Row 4: 1, 2, 3, +
        .with_child(calc_button(data, "1", CalcEvent::Digit('1'), button_style))
        .with_child(calc_button(data, "2", CalcEvent::Digit('2'), button_style))
        .with_child(calc_button(data, "3", CalcEvent::Digit('3'), button_style))
        .with_child(calc_button(data, "+", CalcEvent::Operation(Operation::Add), operator_style))
        // Row 5: 0 (spans 2), ., =
        .with_child(calc_button(data, "0", CalcEvent::Digit('0'), zero_style))
        .with_child(calc_button(data, ".", CalcEvent::Digit('.'), button_style))
        .with_child(calc_button(data, "=", CalcEvent::Equals, operator_style));

    Dom::div()
        .with_inline_style(calculator_style)
        .with_child(display)
        .with_child(buttons)
        .style(Css::empty())
}

fn main() {
    let data = RefAny::new(Calculator::new());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.title = "Calculator - CSS Grid Demo".into();
    options.state.size.dimensions.width = 320.0;
    options.state.size.dimensions.height = 480.0;
    app.run(options);
}
