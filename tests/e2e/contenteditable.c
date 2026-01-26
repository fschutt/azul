/**
 * ContentEditable E2E Test with Large Font
 * 
 * Tests contenteditable text input, cursor movement, selection, and scroll-auto-follow:
 * 1. Single-line contenteditable input
 * 2. Multi-line contenteditable textarea
 * 3. Cursor movement (arrow keys)
 * 4. Text selection (Shift+Arrow, Ctrl+A)
 * 5. Text input (typing characters)
 * 6. Scroll-into-view when cursor moves off-screen
 * 7. Backspace/Delete key handling
 * 
 * Uses LARGE FONT (48px) for easy visual debugging
 * 
 * Compile:
 *   cd tests/e2e && cc contenteditable.c -I../../examples/c -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./contenteditable_test
 * Test with: ./test_contenteditable.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define MAX_TEXT_LEN 4096

typedef struct {
    char single_line_text[256];
    char multi_line_text[MAX_TEXT_LEN];
    int cursor_line;
    int cursor_column;
    int selection_start;
    int selection_end;
    int key_press_count;
    int text_change_count;
} ContentEditableData;

void ContentEditableData_destructor(void* data) {}

AZ_REFLECT(ContentEditableData, ContentEditableData_destructor)

// ============================================================================
// Callbacks
// ============================================================================

// Callback for tracking text input events
AzUpdate on_text_input(AzRefAny data, AzCallbackInfo info) {
    ContentEditableDataRefMut ref = ContentEditableDataRefMut_create(&data);
    if (!ContentEditableData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    
    ref.ptr->text_change_count++;
    ContentEditableDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

// Callback for key press events
AzUpdate on_key_down(AzRefAny data, AzCallbackInfo info) {
    ContentEditableDataRefMut ref = ContentEditableDataRefMut_create(&data);
    if (!ContentEditableData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    
    ref.ptr->key_press_count++;
    ContentEditableDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

// ============================================================================
// CSS Styling (Large Font for Debugging)
// ============================================================================

const char* CSS_STYLE = 
    "body { \n"
    "    display: flex; \n"
    "    flex-direction: column; \n"
    "    padding: 20px; \n"
    "    background-color: #1e1e1e; \n"
    "    font-family: 'Cascadia Code', 'Consolas', monospace; \n"
    "}\n"
    "\n"
    ".label {\n"
    "    font-size: 32px;\n"
    "    color: #cccccc;\n"
    "    margin-bottom: 10px;\n"
    "    margin-top: 20px;\n"
    "}\n"
    "\n"
    ".single-line-input {\n"
    "    font-size: 48px;\n"
    "    padding: 20px;\n"
    "    background-color: #2d2d2d;\n"
    "    color: #ffffff;\n"
    "    border: 3px solid #555555;\n"
    "    min-height: 80px;\n"
    "    cursor: text;\n"
    "}\n"
    "\n"
    ".single-line-input:focus {\n"
    "    border-color: #0078d4;\n"
    "    outline: none;\n"
    "}\n"
    "\n"
    ".multi-line-textarea {\n"
    "    font-size: 48px;\n"
    "    padding: 20px;\n"
    "    background-color: #2d2d2d;\n"
    "    color: #ffffff;\n"
    "    border: 3px solid #555555;\n"
    "    min-height: 300px;\n"
    "    max-height: 400px;\n"
    "    overflow-y: scroll;\n"
    "    cursor: text;\n"
    "    white-space: pre-wrap;\n"
    "    line-height: 1.4;\n"
    "}\n"
    "\n"
    ".multi-line-textarea:focus {\n"
    "    border-color: #0078d4;\n"
    "    outline: none;\n"
    "}\n"
    "\n"
    ".status-bar {\n"
    "    font-size: 24px;\n"
    "    color: #888888;\n"
    "    margin-top: 20px;\n"
    "    padding: 10px;\n"
    "    background-color: #252525;\n"
    "}\n"
    "\n"
    "/* Cursor styling */\n"
    "::cursor {\n"
    "    width: 3px;\n"
    "    background-color: #ffffff;\n"
    "}\n"
    "\n"
    "/* Selection styling */\n"
    "::selection {\n"
    "    background-color: #264f78;\n"
    "}\n";

// ============================================================================
// DOM Layout
// ============================================================================

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    ContentEditableDataRef ref = ContentEditableDataRef_create(&data);
    if (!ContentEditableData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Build DOM
    AzDom root = AzDom_createBody();
    
    // Label 1: Single Line Input
    AzDom label1 = AzDom_createText(AZ_STR("Single Line Input (48px font):"));
    AzDom_addClass(&label1, AZ_STR("label"));
    AzDom_addChild(&root, label1);
    
    // Single-line contenteditable input
    AzDom single_input = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
    AzDom_addClass(&single_input, AZ_STR("single-line-input"));
    AzTabIndex tab_auto = { .Auto = { .tag = AzTabIndex_Tag_Auto } };
    AzDom_setTabIndex(&single_input, tab_auto);
    
    // Add text input callback - use Focus filter for text input
    AzEventFilter text_filter = AzEventFilter_focus(AzFocusEventFilter_TextInput);
    AzDom_addCallback(&single_input, text_filter, AzRefAny_clone(&data), on_text_input);
    
    AzDom_addChild(&root, single_input);
    
    // Label 2: Multi Line Text Area
    AzDom label2 = AzDom_createText(AZ_STR("Multi Line Text Area (scroll test):"));
    AzDom_addClass(&label2, AZ_STR("label"));
    AzDom_addChild(&root, label2);
    
    // Multi-line contenteditable textarea
    AzDom multi_input = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
    AzDom_addClass(&multi_input, AZ_STR("multi-line-textarea"));
    AzDom_setTabIndex(&multi_input, tab_auto);
    
    // Add callbacks
    AzDom_addCallback(&multi_input, text_filter, AzRefAny_clone(&data), on_text_input);
    
    AzEventFilter key_filter = AzEventFilter_focus(AzFocusEventFilter_VirtualKeyDown);
    AzDom_addCallback(&multi_input, key_filter, AzRefAny_clone(&data), on_key_down);
    
    AzDom_addChild(&root, multi_input);
    
    // Status bar
    char status[256];
    snprintf(status, sizeof(status), 
             "Cursor: Line %d, Col %d | Selection: %d-%d | Keys: %d | Changes: %d",
             ref.ptr->cursor_line, ref.ptr->cursor_column,
             ref.ptr->selection_start, ref.ptr->selection_end,
             ref.ptr->key_press_count, ref.ptr->text_change_count);
    
    AzDom status_bar = AzDom_createText(AZ_STR(status));
    AzDom_addClass(&status_bar, AZ_STR("status-bar"));
    AzDom_addChild(&root, status_bar);
    
    ContentEditableDataRef_delete(&ref);
    
    // Parse and apply CSS
    AzCss css = AzCss_fromString(AZ_STR(CSS_STYLE));
    return AzDom_style(&root, css);
}

// ============================================================================
// Main
// ============================================================================

int main(int argc, char** argv) {
    printf("ContentEditable E2E Test\n");
    printf("========================\n");
    printf("Features tested:\n");
    printf("  - Large font (48px) for easy visual debugging\n");
    printf("  - Single-line contenteditable input\n");
    printf("  - Multi-line contenteditable textarea with scroll\n");
    printf("  - Tab navigation between inputs\n");
    printf("  - Text input, cursor movement, selection\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765\n");
    printf("Test commands:\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"get_state\"}'\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"key_down\", \"key\": \"Tab\"}'\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"text_input\", \"text\": \"Hello\"}'\n");
    printf("\n");
    
    // Initialize app data
    ContentEditableData initial = {
        .single_line_text = "Hello World - Click here and type!",
        .multi_line_text = 
           "Line 1: This is a multi-line text area.\n"
           "Line 2: Use arrow keys to move cursor.\n"
           "Line 3: Use Shift+Arrow to select text.\n"
           "Line 4: Use Ctrl+A to select all.\n"
           "Line 5: Type to insert text at cursor.\n"
           "Line 6: Backspace/Delete to remove text.\n"
           "Line 7: This tests scroll-into-view.\n"
           "Line 8: When cursor goes off-screen...\n"
           "Line 9: The view should scroll automatically.\n"
           "Line 10: End of test content.",
        .cursor_line = 1,
        .cursor_column = 0,
        .selection_start = 0,
        .selection_end = 0,
        .key_press_count = 0,
        .text_change_count = 0
    };
    
    AzRefAny data = ContentEditableData_upcast(initial);
    
    // Create window
    AzWindowCreateOptions win_opts = AzWindowCreateOptions_create(layout);
    win_opts.window_state.title = AZ_STR("ContentEditable Test - 48px Font");
    win_opts.window_state.size.dimensions.width = 1200.0;
    win_opts.window_state.size.dimensions.height = 800.0;
    
    // Create app
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, win_opts);
    AzApp_delete(&app);
    
    return 0;
}
