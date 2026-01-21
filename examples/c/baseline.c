/**
 * Baseline Alignment Debug Example
 * 
 * This example tests baseline alignment of inline text with different font sizes.
 * All text nodes within a single container should align by their baselines.
 * 
 * Compile with: 
 *   clang -o baseline baseline.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom root = AzDom_createDiv();
    AzDom_setInlineStyle(&root, az_str("padding: 40px; background-color: #fff;"));
    
    // Single paragraph container - all children should align by baseline
    AzDom para = AzDom_createDiv();
    AzDom_setInlineStyle(&para, az_str("background-color: #f0f0f0; padding: 20px;"));
    
    // Multiple text nodes with different font sizes
    AzDom t1 = AzDom_createText(az_str("Small "));
    AzDom_setInlineStyle(&t1, az_str("font-size: 12px; background-color: #fdd;"));
    AzDom_addChild(&para, t1);
    
    AzDom t2 = AzDom_createText(az_str("LARGE "));
    AzDom_setInlineStyle(&t2, az_str("font-size: 32px; background-color: #dfd;"));
    AzDom_addChild(&para, t2);
    
    AzDom t3 = AzDom_createText(az_str("Medium "));
    AzDom_setInlineStyle(&t3, az_str("font-size: 18px; background-color: #ddf;"));
    AzDom_addChild(&para, t3);
    
    AzDom t4 = AzDom_createText(az_str("tiny "));
    AzDom_setInlineStyle(&t4, az_str("font-size: 10px; background-color: #ffd;"));
    AzDom_addChild(&para, t4);
    
    AzDom t5 = AzDom_createText(az_str("HUGE"));
    AzDom_setInlineStyle(&t5, az_str("font-size: 48px; background-color: #fdf;"));
    AzDom_addChild(&para, t5);
    
    AzDom_addChild(&root, para);
    
    AzCss css = AzCss_empty();
    return AzDom_style(&root, css);
}

int main() {
    printf("Baseline Alignment Debug\n");
    printf("========================\n\n");
    
    AzAppConfig config = AzAppConfig_create();
    
    AzString empty_type = az_str("");
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL, 0, 0);
    
    AzApp app = AzApp_create(empty_data, config);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = az_str("Baseline Alignment Debug");
    window.window_state.size.dimensions.width = 600.0f;
    window.window_state.size.dimensions.height = 200.0f;
    
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
