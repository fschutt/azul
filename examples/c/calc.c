// Calculator with CSS Grid - C
// Demonstrates CSS Grid layout and component composition
// cc -o calc calc.c -lazul

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

typedef enum {
    OP_NONE,
    OP_ADD,
    OP_SUBTRACT,
    OP_MULTIPLY,
    OP_DIVIDE
} Operation;

typedef struct {
    char display[64];
    double current_value;
    Operation pending_operation;
    double pending_value;
    int clear_on_next_input;
} Calculator;

void Calculator_destructor(void* c) { }
AZ_REFLECT(Calculator, Calculator_destructor);

typedef enum {
    EVT_DIGIT,
    EVT_OPERATION,
    EVT_EQUALS,
    EVT_CLEAR,
    EVT_INVERT,
    EVT_PERCENT
} EventType;

typedef struct {
    AzRefAny calc;
    EventType event_type;
    char digit;
    Operation operation;
} ButtonData;

void ButtonData_destructor(void* b_ptr) { 
    ButtonData* b = (ButtonData*)b_ptr;
    AzRefAny_delete(&b->calc);
}
AZ_REFLECT(ButtonData, ButtonData_destructor);

void Calculator_init(Calculator* c) {
    strcpy(c->display, "0");
    c->current_value = 0.0;
    c->pending_operation = OP_NONE;
    c->pending_value = 0.0;
    c->clear_on_next_input = 0;
}

void Calculator_calculate(Calculator* c) {
    if (c->pending_operation == OP_NONE) return;
    
    double result = 0.0;
    switch (c->pending_operation) {
        case OP_ADD: result = c->pending_value + c->current_value; break;
        case OP_SUBTRACT: result = c->pending_value - c->current_value; break;
        case OP_MULTIPLY: result = c->pending_value * c->current_value; break;
        case OP_DIVIDE:
            if (c->current_value != 0.0) {
                result = c->pending_value / c->current_value;
            } else {
                strcpy(c->display, "Error");
                c->pending_operation = OP_NONE;
                return;
            }
            break;
        default: break;
    }
    
    c->current_value = result;
    if (fabs(result - floor(result)) < 0.0000001 && fabs(result) < 1e15) {
        snprintf(c->display, sizeof(c->display), "%lld", (long long)result);
    } else {
        snprintf(c->display, sizeof(c->display), "%g", result);
    }
    c->pending_operation = OP_NONE;
    c->clear_on_next_input = 1;
}

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);

AzDom create_button(AzRefAny* calc, const char* label, EventType evt, char digit, Operation op, const char* style) {
    ButtonData bd;
    bd.calc = AzRefAny_deepCopy(calc);
    bd.event_type = evt;
    bd.digit = digit;
    bd.operation = op;
    
    AzDom button = AzDom_div();
    AzString style_str = AzString_copyFromBytes((const uint8_t*)style, 0, strlen(style));
    AzDom_setInlineStyle(&button, style_str);
    AzString label_str = AzString_copyFromBytes((const uint8_t*)label, 0, strlen(label));
    AzDom_addChild(&button, AzDom_text(label_str));
    AzEventFilter event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_MouseUp } };
    AzDom_addCallback(&button, event, ButtonData_upcast(bd), on_button_click);
    
    return button;
}

static const char* CALC_STYLE = 
    "height:100%;"
    "display:flex;"
    "flex-direction:column;"
    "font-family:sans-serif;";

static const char* DISPLAY_STYLE = 
    "background-color:#2d2d2d;"
    "color:white;"
    "font-size:48px;"
    "text-align:right;"
    "padding:20px;"
    "display:flex;"
    "align-items:center;"
    "justify-content:flex-end;"
    "min-height:80px;";

static const char* BUTTONS_STYLE = 
    "flex-grow:1;"
    "display:grid;"
    "grid-template-columns:1fr 1fr 1fr 1fr;"
    "grid-template-rows:1fr 1fr 1fr 1fr 1fr;"
    "gap:1px;"
    "background-color:#666666;";

static const char* BTN_STYLE = 
    "background-color:#d1d1d6;"
    "color:#1d1d1f;"
    "font-size:24px;"
    "display:flex;"
    "align-items:center;"
    "justify-content:center;";

static const char* OP_STYLE = 
    "background-color:#ff9f0a;"
    "color:white;"
    "font-size:24px;"
    "display:flex;"
    "align-items:center;"
    "justify-content:center;";

static const char* ZERO_STYLE = 
    "background-color:#d1d1d6;"
    "color:#1d1d1f;"
    "font-size:24px;"
    "display:flex;"
    "align-items:center;"
    "justify-content:flex-start;"
    "padding-left:28px;"
    "grid-column:span 2;";

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    CalculatorRef c = CalculatorRef_create(&data);
    if (!Calculator_downcastRef(&data, &c)) {
        return AzStyledDom_default();
    }
    
    char display_text[64];
    strcpy(display_text, c.ptr->display);
    CalculatorRef_delete(&c);

    // Display
    AzDom display = AzDom_div();
    AzString display_style = AzString_copyFromBytes((const uint8_t*)DISPLAY_STYLE, 0, strlen(DISPLAY_STYLE));
    AzDom_setInlineStyle(&display, display_style);
    AzDom_addChild(&display, AzDom_text(AzString_copyFromBytes((uint8_t*)display_text, 0, strlen(display_text))));

    // Buttons grid
    AzDom buttons = AzDom_div();
    AzString buttons_style = AzString_copyFromBytes((const uint8_t*)BUTTONS_STYLE, 0, strlen(BUTTONS_STYLE));
    AzDom_setInlineStyle(&buttons, buttons_style);
    
    // Row 1
    AzDom_addChild(&buttons, create_button(&data, "C", EVT_CLEAR, 0, OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "+/-", EVT_INVERT, 0, OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "%", EVT_PERCENT, 0, OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "\xc3\xb7", EVT_OPERATION, 0, OP_DIVIDE, OP_STYLE));
    
    // Row 2
    AzDom_addChild(&buttons, create_button(&data, "7", EVT_DIGIT, '7', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "8", EVT_DIGIT, '8', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "9", EVT_DIGIT, '9', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "\xc3\x97", EVT_OPERATION, 0, OP_MULTIPLY, OP_STYLE));
    
    // Row 3
    AzDom_addChild(&buttons, create_button(&data, "4", EVT_DIGIT, '4', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "5", EVT_DIGIT, '5', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "6", EVT_DIGIT, '6', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "-", EVT_OPERATION, 0, OP_SUBTRACT, OP_STYLE));
    
    // Row 4
    AzDom_addChild(&buttons, create_button(&data, "1", EVT_DIGIT, '1', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "2", EVT_DIGIT, '2', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "3", EVT_DIGIT, '3', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "+", EVT_OPERATION, 0, OP_ADD, OP_STYLE));
    
    // Row 5
    AzDom_addChild(&buttons, create_button(&data, "0", EVT_DIGIT, '0', OP_NONE, ZERO_STYLE));
    AzDom_addChild(&buttons, create_button(&data, ".", EVT_DIGIT, '.', OP_NONE, BTN_STYLE));
    AzDom_addChild(&buttons, create_button(&data, "=", EVT_EQUALS, 0, OP_NONE, OP_STYLE));

    // Main container
    AzDom body = AzDom_div();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)CALC_STYLE, 0, strlen(CALC_STYLE));
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, display);
    AzDom_addChild(&body, buttons);

    return AzStyledDom_new(body, AzCss_empty());
}

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info) {
    ButtonDataRef bd = ButtonDataRef_create(&data);
    if (!ButtonData_downcastRef(&data, &bd)) {
        return AzUpdate_DoNothing;
    }
    
    AzRefAny calc_ref = AzRefAny_clone(&bd.ptr->calc);
    EventType evt = bd.ptr->event_type;
    char digit = bd.ptr->digit;
    Operation op = bd.ptr->operation;
    ButtonDataRef_delete(&bd);
    
    CalculatorRefMut c = CalculatorRefMut_create(&calc_ref);
    if (!Calculator_downcastMut(&calc_ref, &c)) {
        AzRefAny_delete(&calc_ref);
        return AzUpdate_DoNothing;
    }
    
    switch (evt) {
        case EVT_DIGIT:
            if (c.ptr->clear_on_next_input) {
                c.ptr->display[0] = '\0';
                c.ptr->clear_on_next_input = 0;
            }
            if (strcmp(c.ptr->display, "0") == 0 && digit != '.') {
                c.ptr->display[0] = digit;
                c.ptr->display[1] = '\0';
            } else if (digit == '.' && strchr(c.ptr->display, '.') != NULL) {
                // Already has decimal
            } else {
                size_t len = strlen(c.ptr->display);
                if (len < 63) {
                    c.ptr->display[len] = digit;
                    c.ptr->display[len + 1] = '\0';
                }
            }
            c.ptr->current_value = atof(c.ptr->display);
            break;
            
        case EVT_OPERATION:
            Calculator_calculate(c.ptr);
            c.ptr->pending_operation = op;
            c.ptr->pending_value = c.ptr->current_value;
            c.ptr->clear_on_next_input = 1;
            break;
            
        case EVT_EQUALS:
            Calculator_calculate(c.ptr);
            break;
            
        case EVT_CLEAR:
            Calculator_init(c.ptr);
            break;
            
        case EVT_INVERT:
            c.ptr->current_value = -c.ptr->current_value;
            snprintf(c.ptr->display, sizeof(c.ptr->display), "%g", c.ptr->current_value);
            break;
            
        case EVT_PERCENT:
            c.ptr->current_value /= 100.0;
            snprintf(c.ptr->display, sizeof(c.ptr->display), "%g", c.ptr->current_value);
            break;
    }
    
    CalculatorRefMut_delete(&c);
    AzRefAny_delete(&calc_ref);
    
    return AzUpdate_RefreshDom;
}

int main() {
    Calculator model;
    Calculator_init(&model);
    AzRefAny data = Calculator_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Calculator - CSS Grid Demo");
    window.state.size.dimensions.width = 320.0;
    window.state.size.dimensions.height = 480.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
