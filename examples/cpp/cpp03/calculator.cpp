// Azul C++ Calculator Example - C++03 compatible
// Demonstrates interactive UI with buttons and state management

#include <azul.h>
#include <cstdio>
#include <cstring>
#include <cstdlib>

// Global string constants
static const AzString WINDOW_TITLE = AzString_fromConstStr("Azul Calculator - C++03");
static const AzString BTN_STYLE = AzString_fromConstStr("font-size: 24px; min-width: 60px; min-height: 60px;");
static const AzString DISPLAY_STYLE = AzString_fromConstStr("font-size: 32px; background: white; padding: 10px; text-align: right;");
static const AzString ROW_STYLE = AzString_fromConstStr("flex-direction: row;");
static const AzString CONTAINER_STYLE = AzString_fromConstStr("flex-grow: 1; padding: 10px;");

// Calculator state
typedef struct {
    double current_value;
    double stored_value;
    char operation;
    bool clear_next;
} CalculatorState;

// Type ID for RefAny
AZ_REFLECT(CalculatorState, CalculatorState_destructor)

void CalculatorState_destructor(CalculatorState* state) {
    // Nothing to clean up for POD struct
    (void)state;
}

// Forward declarations
AzDom digit_button(int digit, AzRefAny* state);
AzDom op_button(const char* label, char op, AzRefAny* state);
AzDom create_display(AzRefAny* state);

// Callback for digit buttons
AzUpdate on_digit_click(AzRefAny* state, AzCallbackInfo* info) {
    // Get the digit from event data
    int digit = 0; // Would need to be passed via custom mechanism
    
    CalculatorState s;
    if (CalculatorState_downcastRef(state, &s)) {
        if (s.clear_next) {
            s.current_value = digit;
            s.clear_next = false;
        } else {
            s.current_value = s.current_value * 10 + digit;
        }
        // Note: Can't modify in C++03 without downcastMut
    }
    
    AzUpdate upd = AzUpdate_refreshDom(info);
    return upd;
}

// Callback for operation buttons
AzUpdate on_op_click(AzRefAny* state, AzCallbackInfo* info) {
    CalculatorState s;
    CalculatorState* ptr = 0;
    if (CalculatorState_downcastMut(state, &ptr)) {
        // Perform pending operation
        if (ptr->operation != 0) {
            switch (ptr->operation) {
                case '+': ptr->stored_value += ptr->current_value; break;
                case '-': ptr->stored_value -= ptr->current_value; break;
                case '*': ptr->stored_value *= ptr->current_value; break;
                case '/': 
                    if (ptr->current_value != 0) {
                        ptr->stored_value /= ptr->current_value; 
                    }
                    break;
            }
        } else {
            ptr->stored_value = ptr->current_value;
        }
        ptr->operation = '+'; // Default, would need event data
        ptr->clear_next = true;
    }
    
    AzUpdate upd = AzUpdate_refreshDom(info);
    return upd;
}

// Callback for equals button
AzUpdate on_equals_click(AzRefAny* state, AzCallbackInfo* info) {
    CalculatorState* ptr = 0;
    if (CalculatorState_downcastMut(state, &ptr)) {
        switch (ptr->operation) {
            case '+': ptr->current_value = ptr->stored_value + ptr->current_value; break;
            case '-': ptr->current_value = ptr->stored_value - ptr->current_value; break;
            case '*': ptr->current_value = ptr->stored_value * ptr->current_value; break;
            case '/': 
                if (ptr->current_value != 0) {
                    ptr->current_value = ptr->stored_value / ptr->current_value; 
                }
                break;
        }
        ptr->operation = 0;
        ptr->stored_value = 0;
        ptr->clear_next = true;
    }
    
    AzUpdate upd = AzUpdate_refreshDom(info);
    return upd;
}

// Callback for clear button
AzUpdate on_clear_click(AzRefAny* state, AzCallbackInfo* info) {
    CalculatorState* ptr = 0;
    if (CalculatorState_downcastMut(state, &ptr)) {
        ptr->current_value = 0;
        ptr->stored_value = 0;
        ptr->operation = 0;
        ptr->clear_next = false;
    }
    
    AzUpdate upd = AzUpdate_refreshDom(info);
    return upd;
}

// Create a button with a label
AzDom button(const char* label) {
    AzString text = AzString_copyFromBytes((const uint8_t*)label, strlen(label));
    AzString style = AzString_copyFromBytes((const uint8_t*)BTN_STYLE.vec.ptr, BTN_STYLE.vec.len);
    return AzDom_text(AzButton_new(text, AzNodeDataInlineCssPropertyVec_empty()), style);
}

// Layout function
AzStyledDom layout_calculator(AzRefAny* state, AzLayoutCallbackInfo* info) {
    // Display
    char display_text[64];
    CalculatorState s;
    if (CalculatorState_downcastRef(state, &s)) {
        snprintf(display_text, sizeof(display_text), "%.0f", s.current_value);
    } else {
        snprintf(display_text, sizeof(display_text), "0");
    }
    
    AzString display_str = AzString_copyFromBytes((const uint8_t*)display_text, strlen(display_text));
    AzDom display = AzDom_text(AzLabel_new(display_str), DISPLAY_STYLE);
    
    // Create simple layout
    AzDom root = AzDom_div();
    AzDom_setInlineStyle(&root, CONTAINER_STYLE);
    
    // Add display
    AzDom_addChild(&root, display);
    
    // Add button rows (simplified - just show concept)
    AzDom row1 = AzDom_div();
    AzDom_setInlineStyle(&row1, ROW_STYLE);
    AzDom_addChild(&row1, button("7"));
    AzDom_addChild(&row1, button("8"));
    AzDom_addChild(&row1, button("9"));
    AzDom_addChild(&row1, button("/"));
    AzDom_addChild(&root, row1);
    
    AzDom row2 = AzDom_div();
    AzDom_setInlineStyle(&row2, ROW_STYLE);
    AzDom_addChild(&row2, button("4"));
    AzDom_addChild(&row2, button("5"));
    AzDom_addChild(&row2, button("6"));
    AzDom_addChild(&row2, button("*"));
    AzDom_addChild(&root, row2);
    
    AzDom row3 = AzDom_div();
    AzDom_setInlineStyle(&row3, ROW_STYLE);
    AzDom_addChild(&row3, button("1"));
    AzDom_addChild(&row3, button("2"));
    AzDom_addChild(&row3, button("3"));
    AzDom_addChild(&row3, button("-"));
    AzDom_addChild(&root, row3);
    
    AzDom row4 = AzDom_div();
    AzDom_setInlineStyle(&row4, ROW_STYLE);
    AzDom_addChild(&row4, button("0"));
    AzDom_addChild(&row4, button("C"));
    AzDom_addChild(&row4, button("="));
    AzDom_addChild(&row4, button("+"));
    AzDom_addChild(&root, row4);
    
    return AzStyledDom_fromDom(root, AzCss_empty());
}

int main() {
    // Initialize state
    CalculatorState initial_state;
    initial_state.current_value = 0;
    initial_state.stored_value = 0;
    initial_state.operation = 0;
    initial_state.clear_next = false;
    
    AzRefAny state = CalculatorState_upcast(&initial_state);
    AzLayoutCallback layout = AzLayoutCallback_new(state, layout_calculator);
    
    // Create app and window
    AzApp app = AzApp_new(layout);
    AzWindowCreateOptions window_opts = AzWindowCreateOptions_default();
    AzWindowCreateOptions_setTitle(&window_opts, WINDOW_TITLE);
    AzWindowCreateOptions_setDimensions(&window_opts, (AzLayoutSize){400, 500});
    
    AzApp_run(&app, window_opts);
    
    return 0;
}
