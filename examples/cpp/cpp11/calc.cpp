// g++ -std=c++11 -o calc calc.cpp -lazul

#include "azul11.hpp"
#include <string>
#include <cmath>

using namespace azul;

enum class Operation { None, Add, Subtract, Multiply, Divide };

struct Calculator {
    std::string display;
    double current_value;
    Operation pending_op;
    double pending_value;
    bool clear_next;

    Calculator() : display("0"), current_value(0.0), pending_op(Operation::None), pending_value(0.0), clear_next(false) {}
};

enum class EventType { Digit, Op, Equals, Clear, Invert, Percent };

struct ButtonData {
    AzRefAny calc;
    EventType evt;
    char digit;
    Operation op;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

void Calculator_calculate(Calculator* c) {
    if (c->pending_op == Operation::None) return;
    double result = 0.0;
    switch (c->pending_op) {
        case Operation::Add: result = c->pending_value + c->current_value; break;
        case Operation::Subtract: result = c->pending_value - c->current_value; break;
        case Operation::Multiply: result = c->pending_value * c->current_value; break;
        case Operation::Divide:
            if (c->current_value != 0.0) result = c->pending_value / c->current_value;
            else { c->display = "Error"; c->pending_op = Operation::None; return; }
            break;
        default: break;
    }
    c->current_value = result;
    if (std::fabs(result - std::floor(result)) < 0.0000001)
        c->display = std::to_string(static_cast<long long>(result));
    else
        c->display = std::to_string(result);
    c->pending_op = Operation::None;
    c->clear_next = true;
}

const char* CALC_STYLE = "height:100%;display:flex;flex-direction:column;font-family:sans-serif;";
const char* DISPLAY_STYLE = "background:#2d2d2d;color:white;font-size:48px;text-align:right;padding:20px;min-height:80px;display:flex;align-items:center;justify-content:flex-end;";
const char* BUTTONS_STYLE = "flex-grow:1;display:grid;grid-template-columns:1fr 1fr 1fr 1fr;grid-template-rows:1fr 1fr 1fr 1fr 1fr;gap:1px;background:#666;";
const char* BTN_STYLE = "background:#d1d1d6;color:#1d1d1f;font-size:24px;display:flex;align-items:center;justify-content:center;";
const char* OP_STYLE = "background:#ff9f0a;color:white;font-size:24px;display:flex;align-items:center;justify-content:center;";
const char* ZERO_STYLE = "background:#d1d1d6;color:#1d1d1f;font-size:24px;display:flex;align-items:center;justify-content:flex-start;padding-left:28px;grid-column:span 2;";

Dom make_button(RefAny& calc, const char* label, EventType evt, char digit, Operation op, const char* style) {
    ButtonData bd;
    bd.calc = calc.clone().release();
    bd.evt = evt;
    bd.digit = digit;
    bd.op = op;

    Dom btn = Dom::create_div();
    btn.set_css(String(style));
    btn.add_child(Dom::create_text(String(label)));
    btn.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), RefAny::create(std::move(bd)), on_click);
    return btn;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const Calculator* c = data_wrapper.downcast_ref<Calculator>();
    if (!c) return AzDom_createBody();

    Dom display = Dom::create_div();
    display.set_css(String(DISPLAY_STYLE));
    display.add_child(Dom::create_text(String(c->display.c_str())));

    Dom buttons = Dom::create_div();
    buttons.set_css(String(BUTTONS_STYLE));

    // Row 1-5
    buttons.add_child(make_button(data_wrapper, "C", EventType::Clear, 0, Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "+/-", EventType::Invert, 0, Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "%", EventType::Percent, 0, Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "/", EventType::Op, 0, Operation::Divide, OP_STYLE));
    buttons.add_child(make_button(data_wrapper, "7", EventType::Digit, '7', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "8", EventType::Digit, '8', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "9", EventType::Digit, '9', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "x", EventType::Op, 0, Operation::Multiply, OP_STYLE));
    buttons.add_child(make_button(data_wrapper, "4", EventType::Digit, '4', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "5", EventType::Digit, '5', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "6", EventType::Digit, '6', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "-", EventType::Op, 0, Operation::Subtract, OP_STYLE));
    buttons.add_child(make_button(data_wrapper, "1", EventType::Digit, '1', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "2", EventType::Digit, '2', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "3", EventType::Digit, '3', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "+", EventType::Op, 0, Operation::Add, OP_STYLE));
    buttons.add_child(make_button(data_wrapper, "0", EventType::Digit, '0', Operation::None, ZERO_STYLE));
    buttons.add_child(make_button(data_wrapper, ".", EventType::Digit, '.', Operation::None, BTN_STYLE));
    buttons.add_child(make_button(data_wrapper, "=", EventType::Equals, 0, Operation::None, OP_STYLE));

    Dom body = Dom::create_div();
    body.set_css(String(CALC_STYLE));
    body.add_child(std::move(display));
    body.add_child(std::move(buttons));

    return body.style(Css::empty()).release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    const ButtonData* bd = data_wrapper.downcast_ref<ButtonData>();
    if (!bd) return AzUpdate_DoNothing;

    RefAny calc_wrapper(AzRefAny_clone(&bd->calc));
    Calculator* c = calc_wrapper.downcast_mut<Calculator>();
    if (!c) return AzUpdate_DoNothing;

    switch (bd->evt) {
        case EventType::Digit:
            if (c->clear_next) { c->display.clear(); c->clear_next = false; }
            if (c->display == "0" && bd->digit != '.') c->display = std::string(1, bd->digit);
            else if (bd->digit == '.' && c->display.find('.') != std::string::npos) { }
            else c->display += bd->digit;
            c->current_value = std::stod(c->display);
            break;
        case EventType::Op:
            Calculator_calculate(c);
            c->pending_op = bd->op;
            c->pending_value = c->current_value;
            c->clear_next = true;
            break;
        case EventType::Equals: Calculator_calculate(c); break;
        case EventType::Clear:
            c->display = "0"; c->current_value = 0; c->pending_op = Operation::None;
            c->pending_value = 0; c->clear_next = false;
            break;
        case EventType::Invert:
            c->current_value = -c->current_value;
            c->display = std::to_string(c->current_value);
            break;
        case EventType::Percent:
            c->current_value /= 100.0;
            c->display = std::to_string(c->current_value);
            break;
    }
    return AzUpdate_RefreshDom;
}

int main() {
    Calculator model;
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
