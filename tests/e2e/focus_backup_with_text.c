/**
 * Focus & Tab Navigation E2E Test
 * 
 * This example creates focusable buttons to test:
 * 1. Tab key navigation between focusable elements
 * 2. Shift+Tab for reverse navigation
 * 3. Enter/Space key activation (triggers click callback)
 * 4. Escape key to clear focus
 * 5. :focus CSS pseudo-class styling
 * 
 * Run with: AZUL_DEBUG=8765 ./focus
 * Test with: curl -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "Tab"}'
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int click_count_button1;
    int click_count_button2;
    int click_count_button3;
    int last_clicked_button;
} FocusTestData;

void FocusTestData_destructor(void* data) {
    // Nothing to free
}

AZ_REFLECT(FocusTestData, FocusTestData_destructor);

// Callback for Button 1
AzUpdate on_button1_click(AzRefAny data, AzCallbackInfo info) {
    fprintf(stderr, "[DEBUG C] on_button1_click called\n");
    fflush(stderr);
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button1++;
        d.ptr->last_clicked_button = 1;
        fprintf(stderr, "Button 1 clicked! Total: %d\n", d.ptr->click_count_button1);
        fflush(stderr);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    fprintf(stderr, "[DEBUG C] downcastMut FAILED!\n");
    fflush(stderr);
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Callback for Button 2
AzUpdate on_button2_click(AzRefAny data, AzCallbackInfo info) {
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button2++;
        d.ptr->last_clicked_button = 2;
        printf("Button 2 clicked! Total: %d\n", d.ptr->click_count_button2);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Callback for Button 3
AzUpdate on_button3_click(AzRefAny data, AzCallbackInfo info) {
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button3++;
        d.ptr->last_clicked_button = 3;
        printf("Button 3 clicked! Total: %d\n", d.ptr->click_count_button3);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Create a focusable button
AzDom create_button(const char* label, int button_num, AzCallbackType click_callback, AzRefAny data) {
    AzString text = AzString_copyFromBytes((const uint8_t*)label, 0, strlen(label));
    AzDom button = AzDom_createDiv();
    AzDom_addChild(&button, AzDom_createText(text));
    
    // Add click callback - use leftMouseUp for click
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_leftMouseUp());
    AzDom_addCallback(&button, event, AzRefAny_clone(&data), click_callback);
    
    // Make focusable with tabindex=0 (Auto)
    AzDom_setTabIndex(&button, AzTabIndex_auto());
    
    // Style the button with :focus pseudo-class support
    // We use a CSS class to apply :focus styles
    char class_name[32];
    snprintf(class_name, sizeof(class_name), "btn btn-%d", button_num);
    AzString class_str = AzString_copyFromBytes((const uint8_t*)class_name, 0, strlen(class_name));
    AzDom_addClass(&button, class_str);
    
    // Base button style
    AzString style = AzString_copyFromBytes((const uint8_t*)
        "padding: 15px 30px; margin: 10px; background-color: #4a90d9; color: white; "
        "font-size: 18px; font-weight: bold; border-radius: 8px; cursor: pointer; "
        "border: 3px solid transparent; transition: all 0.2s;", 0, 203);
    AzDom_setInlineStyle(&button, style);
    
    return button;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    FocusTestDataRef d = FocusTestDataRef_create(&data);
    if (!FocusTestData_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    
    int click1 = d.ptr->click_count_button1;
    int click2 = d.ptr->click_count_button2;
    int click3 = d.ptr->click_count_button3;
    int last_clicked = d.ptr->last_clicked_button;
    FocusTestDataRef_delete(&d);
    
    // Create header
    AzString header_text = AzString_copyFromBytes((const uint8_t*)"Focus & Tab Navigation Test", 0, 28);
    AzDom header = AzDom_createDiv();
    AzDom_addChild(&header, AzDom_createText(header_text));
    AzString header_style = AzString_copyFromBytes((const uint8_t*)
        "padding: 20px; background-color: #2c3e50; color: white; "
        "font-size: 28px; font-weight: bold; text-align: center;", 0, 111);
    AzDom_setInlineStyle(&header, header_style);
    
    // Create instructions
    AzString instructions_text = AzString_copyFromBytes((const uint8_t*)
        "Press Tab to navigate between buttons. Press Enter or Space to activate. Press Escape to clear focus.", 0, 102);
    AzDom instructions = AzDom_createDiv();
    AzDom_addChild(&instructions, AzDom_createText(instructions_text));
    AzString instructions_style = AzString_copyFromBytes((const uint8_t*)
        "padding: 15px; background-color: #ecf0f1; color: #2c3e50; "
        "font-size: 16px; text-align: center; border-bottom: 1px solid #bdc3c7;", 0, 126);
    AzDom_setInlineStyle(&instructions, instructions_style);
    
    // Create button container
    AzDom button_container = AzDom_createDiv();
    AzString container_style = AzString_copyFromBytes((const uint8_t*)
        "display: flex; flex-direction: row; justify-content: center; "
        "align-items: center; padding: 40px; gap: 20px;", 0, 109);
    AzDom_setInlineStyle(&button_container, container_style);
    
    // Create three buttons
    AzDom btn1 = create_button("Button 1", 1, on_button1_click, data);
    AzDom btn2 = create_button("Button 2", 2, on_button2_click, data);
    AzDom btn3 = create_button("Button 3", 3, on_button3_click, data);
    
    AzDom_addChild(&button_container, btn1);
    AzDom_addChild(&button_container, btn2);
    AzDom_addChild(&button_container, btn3);
    
    // Create status display
    char status_buf[256];
    int status_len = snprintf(status_buf, sizeof(status_buf),
        "Clicks: Button1=%d, Button2=%d, Button3=%d | Last clicked: %s",
        click1, click2, click3,
        last_clicked == 0 ? "None" :
        last_clicked == 1 ? "Button 1" :
        last_clicked == 2 ? "Button 2" : "Button 3");
    
    AzString status_text = AzString_copyFromBytes((const uint8_t*)status_buf, 0, status_len);
    AzDom status = AzDom_createDiv();
    AzDom_addChild(&status, AzDom_createText(status_text));
    AzString status_id = AzString_copyFromBytes((const uint8_t*)"status", 0, 6);
    status = AzDom_withId(status, status_id);
    AzString status_style = AzString_copyFromBytes((const uint8_t*)
        "padding: 20px; background-color: #34495e; color: #ecf0f1; "
        "font-size: 16px; text-align: center; font-family: monospace;", 0, 120);
    AzDom_setInlineStyle(&status, status_style);
    
    // Build body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, header);
    AzDom_addChild(&body, instructions);
    AzDom_addChild(&body, button_container);
    AzDom_addChild(&body, status);
    
    // Body style
    AzString body_style = AzString_copyFromBytes((const uint8_t*)
        "display: flex; flex-direction: column; height: 100%; "
        "font-family: 'Segoe UI', sans-serif;", 0, 90);
    AzDom_setInlineStyle(&body, body_style);
    
    // CSS for :focus pseudo-class
    // When an element is focused, it gets a bright yellow border
    const char* focus_css = 
        ".btn:focus { border: 3px solid #f1c40f !important; "
        "box-shadow: 0 0 10px #f1c40f; background-color: #3498db !important; }";
    
    AzString css_str = AzString_copyFromBytes((const uint8_t*)focus_css, 0, strlen(focus_css));
    AzCss css = AzCss_fromString(css_str);
    
    return AzDom_style(&body, css);
}

int main(int argc, char** argv) {
    printf("Focus & Tab Navigation E2E Test\n");
    printf("================================\n");
    printf("Tab: Next focusable element\n");
    printf("Shift+Tab: Previous focusable element\n");
    printf("Enter/Space: Activate focused button\n");
    printf("Escape: Clear focus\n");
    printf("\n");
    
    // Check for debug mode
    char* debug_port = getenv("AZUL_DEBUG");
    if (debug_port) {
        printf("Debug API enabled on port %s\n", debug_port);
        printf("Test with: curl -X POST http://localhost:%s/ -d '{\"op\": \"key_down\", \"key\": \"Tab\"}'\n\n", debug_port);
    }
    
    // Initialize data
    FocusTestData initial_data = {
        .click_count_button1 = 0,
        .click_count_button2 = 0,
        .click_count_button3 = 0,
        .last_clicked_button = 0
    };
    
    AzRefAny data = FocusTestData_upcast(initial_data);
    
    // Create window with layout callback
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AzString_copyFromBytes((const uint8_t*)"Focus Test", 0, 10);
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 400.0;
    
    // Create and run app
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
