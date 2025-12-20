// g++ -std=c++03 -o calc calc.cpp -lazul

#include "azul03.hpp"
#include <cstdio>
#include <cstdlib>
#include <cmath>

using namespace azul;

struct Calculator {
    char display[64];
    double current_value;
    int pending_op;
    double pending_value;
    int clear_next;
};
AZ_REFLECT(Calculator);

struct ButtonData {
    AzRefAny calc;
    int evt_type;
    char digit;
    int op;
};
AZ_REFLECT(ButtonData);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

Dom make_button(RefAny& calc, const char* label, int evt, char digit, int op, const char* style) {
    ButtonData bd;
    bd.calc = calc.clone().release();
    bd.evt_type = evt;
    bd.digit = digit;
    bd.op = op;
    
    Dom text = Dom::create_text(String(label));
    Dom btn = Dom::create_div();
    btn.set_inline_style(String(style));
    btn.add_child(text);
    btn.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), ButtonData_upcast(bd), on_click);
    return btn;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const Calculator* c = Calculator_downcast_ref(data_wrapper);
    if (!c) return AzStyledDom_default();
    
    Dom display_text = Dom::create_text(String(c->display));
    Dom display = Dom::create_div();
    display.set_inline_style(String("background:#2d2d2d;color:white;font-size:48px;text-align:right;padding:20px;min-height:80px;"));
    display.add_child(display_text);
    
    Dom buttons = Dom::create_div();
    buttons.set_inline_style(String("flex-grow:1;display:grid;grid-template-columns:1fr 1fr 1fr 1fr;gap:1px;"));
    
    buttons.add_child(make_button(data_wrapper, "C", 4, 0, 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "7", 0, '7', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "8", 0, '8', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "9", 0, '9', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "+", 1, 0, 1, "background:#ff9f0a;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "4", 0, '4', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "5", 0, '5', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "6", 0, '6', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "-", 1, 0, 2, "background:#ff9f0a;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "1", 0, '1', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "2", 0, '2', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "3", 0, '3', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "=", 2, 0, 0, "background:#ff9f0a;font-size:24px;padding:20px;"));
    buttons.add_child(make_button(data_wrapper, "0", 0, '0', 0, "background:#d1d1d6;font-size:24px;padding:20px;"));
    
    Dom body = Dom::create_div();
    body.set_inline_style(String("height:100%;display:flex;flex-direction:column;font-family:sans-serif;"));
    body.add_child(display);
    body.add_child(buttons);
    
    return body.style(Css::empty()).release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    const ButtonData* bd = ButtonData_downcast_ref(data_wrapper);
    if (!bd) return AzUpdate_DoNothing;
    
    RefAny calc_wrapper(AzRefAny_deepCopy(&bd->calc));
    Calculator* c = Calculator_downcast_mut(calc_wrapper);
    if (!c) return AzUpdate_DoNothing;
    
    if (bd->evt_type == 0) {
        if (c->clear_next) {
            c->display[0] = '\0';
            c->clear_next = 0;
        }
        size_t len = strlen(c->display);
        if (len < 63) {
            c->display[len] = bd->digit;
            c->display[len + 1] = '\0';
        }
        c->current_value = atof(c->display);
    } else if (bd->evt_type == 4) {
        strcpy(c->display, "0");
        c->current_value = 0;
        c->pending_op = 0;
        c->pending_value = 0;
        c->clear_next = 0;
    }
    
    return AzUpdate_RefreshDom;
}

int main() {
    Calculator model;
    strcpy(model.display, "0");
    model.current_value = 0.0;
    model.pending_op = 0;
    model.pending_value = 0.0;
    model.clear_next = 0;
    RefAny data = Calculator_upcast(model);
    
    LayoutCallback cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(cb);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Calculator", 0, 10);
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
