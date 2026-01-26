/**
 * Text Input E2E Test (Single-Line)
 * 
 * Minimal single-line text input to test:
 * 1. Focus and cursor appearance
 * 2. Text input via keyboard/API
 * 3. Cursor movement (Left/Right arrows)
 * 4. Backspace/Delete
 * 5. Selection via Shift+Arrow
 * 6. Select All (Ctrl+A / Cmd+A)
 * 
 * Uses large font (48px) for visual debugging.
 * 
 * Compile:
 *   cd tests/e2e && cc text_input.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o text_input -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./text_input
 * Test with: ./test_text_input.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    char text[256];
    int cursor_pos;
    int selection_start;
    int selection_end;
    int key_count;
    int input_count;
} TextInputData;

void TextInputData_destructor(void* data) {}

// JSON serialization for debug API
AzJson TextInputData_toJson(AzRefAny refany);
AzResultRefAnyString TextInputData_fromJson(AzJson json);

AZ_REFLECT_JSON(TextInputData, TextInputData_destructor, TextInputData_toJson, TextInputData_fromJson)

AzJson TextInputData_toJson(AzRefAny refany) {
    TextInputDataRef ref = TextInputDataRef_create(&refany);
    if (!TextInputData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    AzJsonKeyValue entries[6] = {
        AzJsonKeyValue_create(AZ_STR("text"), AzJson_string(AZ_STR(ref.ptr->text))),
        AzJsonKeyValue_create(AZ_STR("cursor_pos"), AzJson_int(ref.ptr->cursor_pos)),
        AzJsonKeyValue_create(AZ_STR("selection_start"), AzJson_int(ref.ptr->selection_start)),
        AzJsonKeyValue_create(AZ_STR("selection_end"), AzJson_int(ref.ptr->selection_end)),
        AzJsonKeyValue_create(AZ_STR("key_count"), AzJson_int(ref.ptr->key_count)),
        AzJsonKeyValue_create(AZ_STR("input_count"), AzJson_int(ref.ptr->input_count))
    };
    
    TextInputDataRef_delete(&ref);
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 6);
    return AzJson_object(vec);
}

AzResultRefAnyString TextInputData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Track key events
AzUpdate on_key_down(AzRefAny data, AzCallbackInfo info) {
    TextInputDataRefMut ref = TextInputDataRefMut_create(&data);
    if (!TextInputData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    ref.ptr->key_count++;
    TextInputDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

// Track text input events
AzUpdate on_text_input(AzRefAny data, AzCallbackInfo info) {
    TextInputDataRefMut ref = TextInputDataRefMut_create(&data);
    if (!TextInputData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    ref.ptr->input_count++;
    TextInputDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    TextInputDataRef ref = TextInputDataRef_create(&data);
    if (!TextInputData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Create input field
    AzDom input = AzDom_createDiv();
    
    // Add text content
    AzDom text_node = AzDom_createText(AZ_STR(ref.ptr->text));
    AzDom_addChild(&input, text_node);
    
    // Make it focusable and contenteditable
    AzDom_setTabIndex(&input, AzTabIndex_auto());
    AzDom_addClass(&input, AZ_STR("input"));
    
    // Add event handlers
    AzEventFilter key_filter = AzEventFilter_focus(AzFocusEventFilter_VirtualKeyDown);
    AzDom_addCallback(&input, key_filter, AzRefAny_clone(&data), on_key_down);
    
    AzEventFilter text_filter = AzEventFilter_focus(AzFocusEventFilter_TextInput);
    AzDom_addCallback(&input, text_filter, AzRefAny_clone(&data), on_text_input);
    
    // Status label
    char status[128];
    snprintf(status, sizeof(status), "Keys: %d | Inputs: %d | Cursor: %d | Sel: %d-%d",
             ref.ptr->key_count, ref.ptr->input_count, ref.ptr->cursor_pos,
             ref.ptr->selection_start, ref.ptr->selection_end);
    
    AzDom status_label = AzDom_createDiv();
    AzDom_addChild(&status_label, AzDom_createText(AZ_STR(status)));
    AzDom_addClass(&status_label, AZ_STR("status"));
    
    TextInputDataRef_delete(&ref);
    
    // Build body
    AzDom body = AzDom_createBody();
    
    // Label
    AzDom label = AzDom_createDiv();
    AzDom_addChild(&label, AzDom_createText(AZ_STR("Single-Line Input (Tab to focus, then type):")));
    AzDom_addClass(&label, AZ_STR("label"));
    AzDom_addChild(&body, label);
    
    AzDom_addChild(&body, input);
    AzDom_addChild(&body, status_label);
    
    // CSS
    const char* css_str = 
        "body { "
        "  background-color: #1e1e1e; "
        "  display: flex; "
        "  flex-direction: column; "
        "  padding: 40px; "
        "  flex-grow: 1; "
        "} "
        ".label { "
        "  font-size: 24px; "
        "  color: #cccccc; "
        "  margin-bottom: 20px; "
        "} "
        ".input { "
        "  font-size: 48px; "
        "  padding: 20px; "
        "  background-color: #2d2d2d; "
        "  color: #ffffff; "
        "  border: 3px solid #555555; "
        "  min-height: 80px; "
        "  min-width: 500px; "
        "  cursor: text; "
        "} "
        ".input:focus { "
        "  border-color: #0078d4; "
        "} "
        ".status { "
        "  font-size: 18px; "
        "  color: #888888; "
        "  margin-top: 20px; "
        "  padding: 10px; "
        "  background-color: #252525; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    printf("Text Input E2E Test\n");
    printf("===================\n");
    printf("Single-line input field for testing:\n");
    printf("  - Tab to focus\n");
    printf("  - Type to insert text\n");
    printf("  - Arrow keys to move cursor\n");
    printf("  - Shift+Arrow to select\n");
    printf("  - Backspace/Delete to remove\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765 ./text_input\n");
    printf("Test: ./test_text_input.sh\n");
    printf("\n");
    
    TextInputData initial_data = {
        .text = "Hello World",
        .cursor_pos = 11,  // At end
        .selection_start = -1,
        .selection_end = -1,
        .key_count = 0,
        .input_count = 0
    };
    
    AzRefAny app_data = TextInputData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Text Input Test");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
