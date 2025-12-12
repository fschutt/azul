# Calculator with CSS Grid - Python
# Demonstrates CSS Grid layout and component composition

from azul import *

class Calculator:
    def __init__(self):
        self.display = "0"
        self.current_value = 0.0
        self.pending_operation = None
        self.pending_value = None
        self.clear_on_next_input = False
    
    def input_digit(self, digit):
        if self.clear_on_next_input:
            self.display = ""
            self.clear_on_next_input = False
        if self.display == "0" and digit != '.':
            self.display = digit
        elif digit == '.' and '.' in self.display:
            pass  # Don't add another decimal
        else:
            self.display += digit
        self.current_value = float(self.display) if self.display else 0.0
    
    def set_operation(self, op):
        self.calculate()
        self.pending_operation = op
        self.pending_value = self.current_value
        self.clear_on_next_input = True
    
    def calculate(self):
        if self.pending_operation is None or self.pending_value is None:
            return
        
        if self.pending_operation == "add":
            result = self.pending_value + self.current_value
        elif self.pending_operation == "subtract":
            result = self.pending_value - self.current_value
        elif self.pending_operation == "multiply":
            result = self.pending_value * self.current_value
        elif self.pending_operation == "divide":
            if self.current_value != 0:
                result = self.pending_value / self.current_value
            else:
                self.display = "Error"
                self.pending_operation = None
                self.pending_value = None
                return
        else:
            return
        
        self.current_value = result
        if result == int(result) and abs(result) < 1e15:
            self.display = str(int(result))
        else:
            self.display = str(result)
        self.pending_operation = None
        self.pending_value = None
        self.clear_on_next_input = True
    
    def clear(self):
        self.display = "0"
        self.current_value = 0.0
        self.pending_operation = None
        self.pending_value = None
        self.clear_on_next_input = False
    
    def invert_sign(self):
        self.current_value = -self.current_value
        if self.current_value == int(self.current_value):
            self.display = str(int(self.current_value))
        else:
            self.display = str(self.current_value)
    
    def percent(self):
        self.current_value /= 100.0
        self.display = str(self.current_value)


# Styles
CALC_STYLE = """
    height: 100%;
    display: flex;
    flex-direction: column;
    font-family: sans-serif;
"""

DISPLAY_STYLE = """
    background-color: #2d2d2d;
    color: white;
    font-size: 48px;
    text-align: right;
    padding: 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    min-height: 80px;
"""

BUTTONS_STYLE = """
    flex-grow: 1;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr 1fr;
    grid-template-rows: 1fr 1fr 1fr 1fr 1fr;
    gap: 1px;
    background-color: #666666;
"""

BTN_STYLE = """
    background-color: #d1d1d6;
    color: #1d1d1f;
    font-size: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
"""

OP_STYLE = """
    background-color: #ff9f0a;
    color: white;
    font-size: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
"""

ZERO_STYLE = """
    background-color: #d1d1d6;
    color: #1d1d1f;
    font-size: 24px;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    padding-left: 28px;
    grid-column: span 2;
"""


def create_button(calc_ref, label, event_type, event_data, style):
    def on_click(data, info):
        calc = data.downcast_ref()
        if calc is None:
            return Update.DoNothing
        
        if event_type == "digit":
            calc.input_digit(event_data)
        elif event_type == "operation":
            calc.set_operation(event_data)
        elif event_type == "equals":
            calc.calculate()
        elif event_type == "clear":
            calc.clear()
        elif event_type == "invert":
            calc.invert_sign()
        elif event_type == "percent":
            calc.percent()
        
        return Update.RefreshDom
    
    button = Dom.div()
    button.set_inline_style(style)
    button.add_child(Dom.text(label))
    button.set_callback(On.MouseUp, calc_ref.clone(), on_click)
    return button


def layout(data, info):
    calc = data.downcast_ref()
    if calc is None:
        return StyledDom.default()
    
    display_text = calc.display
    
    # Display
    display = Dom.div()
    display.set_inline_style(DISPLAY_STYLE)
    display.add_child(Dom.text(display_text))
    
    # Buttons grid
    buttons = Dom.div()
    buttons.set_inline_style(BUTTONS_STYLE)
    
    # Row 1
    buttons.add_child(create_button(data, "C", "clear", None, BTN_STYLE))
    buttons.add_child(create_button(data, "+/-", "invert", None, BTN_STYLE))
    buttons.add_child(create_button(data, "%", "percent", None, BTN_STYLE))
    buttons.add_child(create_button(data, "÷", "operation", "divide", OP_STYLE))
    
    # Row 2
    buttons.add_child(create_button(data, "7", "digit", "7", BTN_STYLE))
    buttons.add_child(create_button(data, "8", "digit", "8", BTN_STYLE))
    buttons.add_child(create_button(data, "9", "digit", "9", BTN_STYLE))
    buttons.add_child(create_button(data, "×", "operation", "multiply", OP_STYLE))
    
    # Row 3
    buttons.add_child(create_button(data, "4", "digit", "4", BTN_STYLE))
    buttons.add_child(create_button(data, "5", "digit", "5", BTN_STYLE))
    buttons.add_child(create_button(data, "6", "digit", "6", BTN_STYLE))
    buttons.add_child(create_button(data, "-", "operation", "subtract", OP_STYLE))
    
    # Row 4
    buttons.add_child(create_button(data, "1", "digit", "1", BTN_STYLE))
    buttons.add_child(create_button(data, "2", "digit", "2", BTN_STYLE))
    buttons.add_child(create_button(data, "3", "digit", "3", BTN_STYLE))
    buttons.add_child(create_button(data, "+", "operation", "add", OP_STYLE))
    
    # Row 5
    buttons.add_child(create_button(data, "0", "digit", "0", ZERO_STYLE))
    buttons.add_child(create_button(data, ".", "digit", ".", BTN_STYLE))
    buttons.add_child(create_button(data, "=", "equals", None, OP_STYLE))
    
    # Main container
    body = Dom.div()
    body.set_inline_style(CALC_STYLE)
    body.add_child(display)
    body.add_child(buttons)
    
    return StyledDom.new(body, Css.empty())


def main():
    calc = Calculator()
    data = RefAny.new(calc)
    
    app = App.new(data, AppConfig.default())
    
    window = WindowCreateOptions.new(layout)
    window.state.title = "Calculator - CSS Grid Demo"
    window.state.size.dimensions.width = 320.0
    window.state.size.dimensions.height = 480.0
    
    app.run(window)


if __name__ == "__main__":
    main()
