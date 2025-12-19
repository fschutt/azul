// g++ -std=c++03 -o calc calc.cpp -lazul

#include <azul.hpp>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
using namespace azul;

enum Operation { OP_NONE, OP_ADD, OP_SUBTRACT, OP_MULTIPLY, OP_DIVIDE };

struct Calculator {
    char display[64];
    double current_value;
    Operation pending_op;
    double pending_value;
    int clear_next;
};

void Calculator_destructor(Calculator*) { }
AZ_REFLECT(Calculator, Calculator_destructor);

enum EventType { EVT_DIGIT, EVT_OP, EVT_EQUALS, EVT_CLEAR, EVT_INVERT, EVT_PERCENT };

struct ButtonData {
    RefAny calc;
    EventType evt;
    char digit;
    Operation op;
};

void ButtonData_destructor(ButtonData* b) { }
AZ_REFLECT(ButtonData, ButtonData_destructor);

Update on_click(RefAny& data, CallbackInfo& info);

void Calculator_calculate(Calculator* c) {
    if (c->pending_op == OP_NONE) return;
    double result = 0.0;
    switch (c->pending_op) {
        case OP_ADD: result = c->pending_value + c->current_value; break;
        case OP_SUBTRACT: result = c->pending_value - c->current_value; break;
        case OP_MULTIPLY: result = c->pending_value * c->current_value; break;
        case OP_DIVIDE:
            if (c->current_value != 0.0) result = c->pending_value / c->current_value;
            else { std::strcpy(c->display, "Error"); c->pending_op = OP_NONE; return; }
            break;
        default: break;
    }
    c->current_value = result;
    if (std::fabs(result - std::floor(result)) < 0.0000001)
        std::snprintf(c->display, 64, "%lld", (long long)result);
    else
        std::snprintf(c->display, 64, "%g", result);
    c->pending_op = OP_NONE;
    c->clear_next = 1;
}

static const char* CALC_STYLE = "
    height:100%;
    display:flex;
    flex-direction:column;
    font-family:sans-serif;
";

static const char* DISPLAY_STYLE = "
    background:#2d2d2d;
    color:white;
    font-size:48px;
    text-align:right;
    padding:20px;
    min-height:80px;
    display:flex;
    align-items:center;
    justify-content:flex-end;
";

static const char* BUTTONS_STYLE = "
    flex-grow:1;
    display:grid;
    grid-template-columns:1fr 1fr 1fr 1fr;
    grid-template-rows:1fr 1fr 1fr 1fr 1fr;
    gap:1px;
    background:#666;
";

static const char* BTN_STYLE = "
    background:#d1d1d6;
    color:#1d1d1f;
    font-size:24px;
    display:flex;
    align-items:center;
    justify-content:center;
";

static const char* OP_STYLE = "
    background:#ff9f0a;
    color:white;
    font-size:24px;
    display:flex;
    align-items:center;
    justify-content:center;
";

static const char* ZERO_STYLE = "
    background:#d1d1d6;
    color:#1d1d1f;
    font-size:24px;
    display:flex;
    align-items:center;
    justify-content:flex-start;
    padding-left:28px;
    grid-column:span 2;
";

Dom make_button(
    RefAny& calc, 
    const char* label, 
    EventType evt, 
    char digit, 
    Operation op, 
    const char* style
) {

    ButtonData bd;
    bd.calc = calc.clone();
    bd.evt = evt;
    bd.digit = digit;
    bd.op = op;

    return Dom::create_div()
        .with_inline_style(style)
        .with_child(Dom::create_text(label))
        .with_callback(On::MouseUp, ButtonData::upcast(bd), on_click);
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    CalculatorRef c = CalculatorRef::create(data);
    if (!Calculator::downcastRef(data, c)) return StyledDom::default();
    
    char disp[64];
    std::strcpy(disp, c->display);
    
    Dom display = Dom::create_div().with_inline_style(DISPLAY_STYLE).with_child(Dom::create_text(disp));
    Dom buttons = Dom::create_div().with_inline_style(BUTTONS_STYLE);
    
    // Row 1
    buttons = buttons.with_child(make_button(data, "C", EVT_CLEAR, 0, OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "+/-", EVT_INVERT, 0, OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "%", EVT_PERCENT, 0, OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "/", EVT_OP, 0, OP_DIVIDE, OP_STYLE));
    // Row 2
    buttons = buttons.with_child(make_button(data, "7", EVT_DIGIT, '7', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "8", EVT_DIGIT, '8', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "9", EVT_DIGIT, '9', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "x", EVT_OP, 0, OP_MULTIPLY, OP_STYLE));
    // Row 3
    buttons = buttons.with_child(make_button(data, "4", EVT_DIGIT, '4', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "5", EVT_DIGIT, '5', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "6", EVT_DIGIT, '6', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "-", EVT_OP, 0, OP_SUBTRACT, OP_STYLE));
    // Row 4
    buttons = buttons.with_child(make_button(data, "1", EVT_DIGIT, '1', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "2", EVT_DIGIT, '2', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "3", EVT_DIGIT, '3', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "+", EVT_OP, 0, OP_ADD, OP_STYLE));
    // Row 5
    buttons = buttons.with_child(make_button(data, "0", EVT_DIGIT, '0', OP_NONE, ZERO_STYLE));
    buttons = buttons.with_child(make_button(data, ".", EVT_DIGIT, '.', OP_NONE, BTN_STYLE));
    buttons = buttons.with_child(make_button(data, "=", EVT_EQUALS, 0, OP_NONE, OP_STYLE));
    
    Dom body = Dom::create_div().with_inline_style(CALC_STYLE).with_child(display).with_child(buttons);
    return body.style(Css::empty());
}

Update on_click(RefAny& data, CallbackInfo& info) {
    ButtonDataRef bd = ButtonDataRef::create(data);
    if (!ButtonData::downcastRef(data, bd)) return Update::DoNothing;
    
    RefAny calc_ref = bd->calc.clone();
    EventType evt = bd->evt;
    char digit = bd->digit;
    Operation op = bd->op;
    
    CalculatorRefMut c = CalculatorRefMut::create(calc_ref);
    if (!Calculator::downcastMut(calc_ref, c)) return Update::DoNothing;
    
    switch (evt) {
        case EVT_DIGIT:
            if (c->clear_next) { c->display[0] = '\0'; c->clear_next = 0; }
            if (std::strcmp(c->display, "0") == 0 && digit != '.') {
                c->display[0] = digit; c->display[1] = '\0';
            } else if (digit == '.' && std::strchr(c->display, '.')) {
            } else {
                size_t len = std::strlen(c->display);
                if (len < 63) { c->display[len] = digit; c->display[len+1] = '\0'; }
            }
            c->current_value = std::atof(c->display);
            break;
        case EVT_OP:
            Calculator_calculate(c.ptr);
            c->pending_op = op;
            c->pending_value = c->current_value;
            c->clear_next = 1;
            break;
        case EVT_EQUALS: Calculator_calculate(c.ptr); break;
        case EVT_CLEAR:
            std::strcpy(c->display, "0");
            c->current_value = 0; 
            c->pending_op = OP_NONE; 
            c->pending_value = 0; 
            c->clear_next = 0;
            break;
        case EVT_INVERT:
            c->current_value = -c->current_value;
            std::snprintf(c->display, 64, "%g", c->current_value);
            break;
        case EVT_PERCENT:
            c->current_value /= 100.0;
            std::snprintf(c->display, 64, "%g", c->current_value);
            break;
    }
    return Update::RefreshDom;
}

int main() {
    Calculator model = { "0", 0.0, OP_NONE, 0.0, 0 };
    RefAny data = Calculator::upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("Calculator");
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
    
    return 0;
}
