/**
 * Simple ContentEditable Test
 * 
 * Minimal test: single-line contenteditable that auto-scrolls.
 * Text should never wrap - just expand horizontally.
 * 
 * Compile:
 *   cc simple_contenteditable.c -I../../examples/c -L../../target/debug -lazul -o simple_contenteditable -Wl,-rpath,../../target/debug
 * 
 * Run: AZUL_DEBUG=8765 ./simple_contenteditable
 * Test: ./test_simple_contenteditable.sh
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int dummy;
} AppData;

void AppData_destructor(void* data) {}

AZ_REFLECT(AppData, AppData_destructor)

const char* CSS_STYLE = 
    "body { \n"
    "    padding: 50px; \n"
    "    background-color: #222222; \n"
    "    overflow-x: scroll; \n"
    "}\n"
    "\n"
    ".editor {\n"
    "    font-size: 48px;\n"
    "    font-family: monospace;\n"
    "    padding: 20px;\n"
    "    background-color: #333333;\n"
    "    color: #ffffff;\n"
    "    border: 2px solid #666666;\n"
    "    white-space: nowrap;\n"
    "    overflow-x: visible;\n"
    "    min-width: 100%;\n"
    "    caret-color: #00ff00;\n"
    "}\n";

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom root = AzDom_createBody();
    
    // Single contenteditable div with text
    AzDom editor = AzDom_createDiv();
    AzDom_addClass(&editor, AZ_STR("editor"));
    AzDom_setContenteditable(&editor, true);
    
    // Initial text
    AzDom text = AzDom_createText(AZ_STR("Click here and type..."));
    AzDom_addChild(&editor, text);
    
    AzDom_addChild(&root, editor);
    
    // Parse CSS
    AzString css_string = AZ_STR(CSS_STYLE);
    AzCss css = AzCss_fromString(css_string);
    
    return AzDom_style(&root, css);
}

int main() {
    printf("Simple ContentEditable Test\n");
    printf("===========================\n");
    printf("- Single line, no wrap (white-space: nowrap)\n");
    printf("- Body scrolls to keep cursor in view\n");
    printf("- Green cursor, monospace font\n\n");
    printf("Debug: AZUL_DEBUG=8765\n");
    printf("Click on the text and start typing.\n\n");
    
    AppData model = { .dummy = 0 };
    AzRefAny data = AppData_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString title = AZ_STR("Simple ContentEditable");
    window.window_state.title = title;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
