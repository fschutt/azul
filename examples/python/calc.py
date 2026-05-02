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
        if self.display == "0" and digit != ".":
            self.display = digit
        elif digit == "." and "." in self.display:
            pass
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
        self.display = (str(int(self.current_value))
                        if self.current_value == int(self.current_value)
                        else str(self.current_value))

    def percent(self):
        self.current_value /= 100.0
        self.display = str(self.current_value)


CALC_STYLE = ("height:100%;display:flex;flex-direction:column;"
              "font-family:sans-serif;")
DISPLAY_STYLE = ("background-color:#2d2d2d;color:white;font-size:48px;"
                 "text-align:right;padding:20px;display:flex;align-items:center;"
                 "justify-content:flex-end;min-height:80px;")
BUTTONS_STYLE = ("flex-grow:1;display:grid;"
                 "grid-template-columns:1fr 1fr 1fr 1fr;"
                 "grid-template-rows:1fr 1fr 1fr 1fr 1fr;gap:1px;"
                 "background-color:#666666;")
BTN_STYLE = ("background-color:#d1d1d6;color:#1d1d1f;font-size:24px;"
             "display:flex;align-items:center;justify-content:center;")
OP_STYLE = ("background-color:#ff9f0a;color:white;font-size:24px;"
            "display:flex;align-items:center;justify-content:center;")
ZERO_STYLE = ("background-color:#d1d1d6;color:#1d1d1f;font-size:24px;"
              "display:flex;align-items:center;justify-content:flex-start;"
              "padding-left:28px;grid-column:span 2;")


def make_callback(calc, event_type, event_data):
    def cb(data, info):
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
        return Update.RefreshDom()
    return cb


def button(calc, label, event_type, event_data, style):
    return (Dom.create_div()
            .with_css(style)
            .with_child(Dom.create_text(label))
            .with_callback(
                EventFilter.Hover(HoverEventFilter.MouseUp()),
                calc,
                make_callback(calc, event_type, event_data)))


def layout(data, info):
    display = (Dom.create_div()
               .with_css(DISPLAY_STYLE)
               .with_child(Dom.create_text(data.display)))

    rows = [
        ("C", "clear", None, BTN_STYLE),
        ("+/-", "invert", None, BTN_STYLE),
        ("%", "percent", None, BTN_STYLE),
        ("÷", "operation", "divide", OP_STYLE),
        ("7", "digit", "7", BTN_STYLE),
        ("8", "digit", "8", BTN_STYLE),
        ("9", "digit", "9", BTN_STYLE),
        ("×", "operation", "multiply", OP_STYLE),
        ("4", "digit", "4", BTN_STYLE),
        ("5", "digit", "5", BTN_STYLE),
        ("6", "digit", "6", BTN_STYLE),
        ("-", "operation", "subtract", OP_STYLE),
        ("1", "digit", "1", BTN_STYLE),
        ("2", "digit", "2", BTN_STYLE),
        ("3", "digit", "3", BTN_STYLE),
        ("+", "operation", "add", OP_STYLE),
        ("0", "digit", "0", ZERO_STYLE),
        (".", "digit", ".", BTN_STYLE),
        ("=", "equals", None, OP_STYLE),
    ]

    buttons = Dom.create_div().with_css(BUTTONS_STYLE)
    for label, evt, evt_data, style in rows:
        buttons = buttons.with_child(button(data, label, evt, evt_data, style))

    body = (Dom.create_div()
            .with_css(CALC_STYLE)
            .with_child(display)
            .with_child(buttons))

    return body.style(Css.empty())


def main():
    calc = Calculator()
    app = App.create(calc, AppConfig.create())
    window = WindowCreateOptions.create(layout)
    app.run(window)


if __name__ == "__main__":
    main()
