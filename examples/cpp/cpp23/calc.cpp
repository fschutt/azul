// g++ -std=c++23 -o calc calc.cpp -lazul

#include <azul.hpp>
#include <string>
#include <format>
#include <cmath>

using namespace azul;
using namespace std::string_view_literals;

enum class Operation { None, Add, Subtract, Multiply, Divide };

struct Calculator {
    std::string display{"0"};
    double current_value{0.0};
    Operation pending_op{Operation::None};
    double pending_value{0.0};
    bool clear_next{false};
};

enum class EventType { Digit, Op, Equals, Clear, Invert, Percent };

struct ButtonData {
    RefAny calc;
    EventType evt;
    char digit;
    Operation op;
};

Update on_click(RefAny& data, CallbackInfo& info);

inline void calculate(Calculator* c) {
    if (c->pending_op == Operation::None) return;
    
    auto result = [&]() -> double {
        switch (c->pending_op) {
            case Operation::Add: return c->pending_value + c->current_value;
            case Operation::Subtract: return c->pending_value - c->current_value;
            case Operation::Multiply: return c->pending_value * c->current_value;
            case Operation::Divide:
                return c->current_value != 0.0 ? c->pending_value / c->current_value : NAN;
            default: return 0.0;
        }
    }();
    
    if (std::isnan(result)) { c->display = "Error"; c->pending_op = Operation::None; return; }
    c->current_value = result;
    c->display = std::fabs(result - std::floor(result)) < 0.0000001
        ? std::format("{}", static_cast<long long>(result))
        : std::format("{}", result);
    c->pending_op = Operation::None;
    c->clear_next = true;
}

inline constexpr auto CALC_STYLE = "height:100%;display:flex;flex-direction:column;font-family:sans-serif;"sv;
inline constexpr auto DISPLAY_STYLE = "background:#2d2d2d;color:white;font-size:48px;text-align:right;padding:20px;min-height:80px;display:flex;align-items:center;justify-content:flex-end;"sv;
inline constexpr auto BUTTONS_STYLE = "flex-grow:1;display:grid;grid-template-columns:1fr 1fr 1fr 1fr;grid-template-rows:1fr 1fr 1fr 1fr 1fr;gap:1px;background:#666;"sv;
inline constexpr auto BTN_STYLE = "background:#d1d1d6;color:#1d1d1f;font-size:24px;display:flex;align-items:center;justify-content:center;"sv;
inline constexpr auto OP_STYLE = "background:#ff9f0a;color:white;font-size:24px;display:flex;align-items:center;justify-content:center;"sv;
inline constexpr auto ZERO_STYLE = "background:#d1d1d6;color:#1d1d1f;font-size:24px;display:flex;align-items:center;justify-content:flex-start;padding-left:28px;grid-column:span 2;"sv;

auto make_button(
    RefAny& calc, 
    std::string_view label, 
    EventType evt, 
    char digit, 
    Operation op, 
    std::string_view style
) {
    auto bd = ButtonData{calc.clone(), evt, digit, op};

    return Dom::div()
        .with_inline_style(style)
        .with_child(Dom::text(label))
        .with_callback(On::MouseUp, RefAny::new(bd), on_click);
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto c = Calculator::downcast_ref(data);
    if (!c) return StyledDom::default();
    
    auto display = Dom::div().with_inline_style(DISPLAY_STYLE).with_child(Dom::text(c->display));
    auto buttons = Dom::div().with_inline_style(BUTTONS_STYLE)
        .with_child(make_button(data, "C"sv, EventType::Clear, 0, Operation::None, BTN_STYLE))
        .with_child(make_button(data, "+/-"sv, EventType::Invert, 0, Operation::None, BTN_STYLE))
        .with_child(make_button(data, "%"sv, EventType::Percent, 0, Operation::None, BTN_STYLE))
        .with_child(make_button(data, "/"sv, EventType::Op, 0, Operation::Divide, OP_STYLE))
        .with_child(make_button(data, "7"sv, EventType::Digit, '7', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "8"sv, EventType::Digit, '8', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "9"sv, EventType::Digit, '9', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "x"sv, EventType::Op, 0, Operation::Multiply, OP_STYLE))
        .with_child(make_button(data, "4"sv, EventType::Digit, '4', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "5"sv, EventType::Digit, '5', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "6"sv, EventType::Digit, '6', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "-"sv, EventType::Op, 0, Operation::Subtract, OP_STYLE))
        .with_child(make_button(data, "1"sv, EventType::Digit, '1', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "2"sv, EventType::Digit, '2', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "3"sv, EventType::Digit, '3', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "+"sv, EventType::Op, 0, Operation::Add, OP_STYLE))
        .with_child(make_button(data, "0"sv, EventType::Digit, '0', Operation::None, ZERO_STYLE))
        .with_child(make_button(data, "."sv, EventType::Digit, '.', Operation::None, BTN_STYLE))
        .with_child(make_button(data, "="sv, EventType::Equals, 0, Operation::None, OP_STYLE));
    
    return Dom::div()
        .with_inline_style(CALC_STYLE)
        .with_child(display)
        .with_child(buttons)
        .style(Css::empty());
}

Update on_click(RefAny& data, CallbackInfo& info) {
    auto bd = ButtonData::downcast_ref(data);
    if (!bd) return Update::DoNothing;
    
    auto c = Calculator::downcast_mut(bd->calc);
    if (!c) return Update::DoNothing;
    
    switch (bd->evt) {
        case EventType::Digit:
            if (c->clear_next) { c->display.clear(); c->clear_next = false; }
            if (c->display == "0" && bd->digit != '.') c->display = std::string(1, bd->digit);
            else if (bd->digit == '.' && c->display.find('.') != std::string::npos) { }
            else c->display += bd->digit;
            c->current_value = std::stod(c->display);
            break;
        case EventType::Op:
            calculate(c.ptr);
            c->pending_op = bd->op;
            c->pending_value = c->current_value;
            c->clear_next = true;
            break;
        case EventType::Equals: calculate(c.ptr); break;
        case EventType::Clear:
            c->display = "0"; c->current_value = 0; c->pending_op = Operation::None;
            c->pending_value = 0; c->clear_next = false;
            break;
        case EventType::Invert:
            c->current_value = -c->current_value;
            c->display = std::format("{}", c->current_value);
            break;
        case EventType::Percent:
            c->current_value /= 100.0;
            c->display = std::format("{}", c->current_value);
            break;
    }
    return Update::RefreshDom;
}

int main() {
    auto data = RefAny::new(Calculator{.display = "0"});
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Calculator"sv);
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
