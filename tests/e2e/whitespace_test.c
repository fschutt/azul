/**
 * White-space CSS Property E2E Test
 * 
 * Tests different white-space values:
 * 1. white-space: nowrap - should NOT wrap at word boundaries
 * 2. white-space: pre - should preserve newlines and NOT wrap
 * 3. white-space: normal - should wrap at word boundaries
 * 
 * Compile:
 *   cd tests/e2e && cc whitespace_test.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o whitespace_test -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./whitespace_test
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int dummy;
} AppData;

void AppData_destructor(void* data) {}

AZ_REFLECT(AppData, AppData_destructor)

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Container
    AzDom body = AzDom_createBody();
    
    // ========== Test 1: white-space: nowrap ==========
    AzDom label1 = AzDom_createDiv();
    AzDom_addChild(&label1, AzDom_createText(AZ_STR("1. white-space: nowrap (single line, clipped):")));
    AzDom_addClass(&label1, AZ_STR("label"));
    AzDom_addChild(&body, label1);
    
    AzDom nowrap_div = AzDom_createDiv();
    AzDom_addChild(&nowrap_div, AzDom_createText(AZ_STR("This is a very long line that should never wrap at word boundaries because white-space is set to nowrap")));
    AzDom_addClass(&nowrap_div, AZ_STR("nowrap-box"));
    AzDom_addChild(&body, nowrap_div);
    
    // ========== Test 2: white-space: pre ==========
    AzDom label2 = AzDom_createDiv();
    AzDom_addChild(&label2, AzDom_createText(AZ_STR("2. white-space: pre (5 lines from \\n):")));
    AzDom_addClass(&label2, AZ_STR("label"));
    AzDom_addChild(&body, label2);
    
    AzDom pre_div = AzDom_createDiv();
    AzDom_addChild(&pre_div, AzDom_createText(AZ_STR("Line 1\nLine 2\nLine 3\nLine 4\nLine 5")));
    AzDom_addClass(&pre_div, AZ_STR("pre-box"));
    AzDom_addChild(&body, pre_div);
    
    // ========== Test 3: white-space: normal ==========
    AzDom label3 = AzDom_createDiv();
    AzDom_addChild(&label3, AzDom_createText(AZ_STR("3. white-space: normal (wraps at words):")));
    AzDom_addClass(&label3, AZ_STR("label"));
    AzDom_addChild(&body, label3);
    
    AzDom normal_div = AzDom_createDiv();
    AzDom_addChild(&normal_div, AzDom_createText(AZ_STR("This is a very long line that should wrap at word boundaries because white-space is normal")));
    AzDom_addClass(&normal_div, AZ_STR("normal-box"));
    AzDom_addChild(&body, normal_div);
    
    // CSS
    const char* css_str = 
        "body { "
        "  background-color: #1e1e1e; "
        "  padding: 20px; "
        "  flex-grow: 1; "
        "} "
        ".label { "
        "  margin-top: 15px; "
        "  margin-bottom: 5px; "
        "  font-weight: bold; "
        "  color: #cccccc; "
        "} "
        ".nowrap-box { "
        "  width: 200px; "
        "  height: 50px; "
        "  white-space: nowrap; "
        "  overflow: hidden; "
        "  border: 2px solid #4444ff; "
        "  font-size: 14px; "
        "  color: #ffffff; "
        "  background-color: #2d2d2d; "
        "  padding: 5px; "
        "} "
        ".pre-box { "
        "  width: 200px; "
        "  height: 140px; "
        "  white-space: pre; "
        "  overflow: auto; "
        "  border: 2px solid #44ff44; "
        "  font-size: 16px; "
        "  line-height: 1.4; "
        "  color: #ffffff; "
        "  background-color: #2d2d2d; "
        "  padding: 5px; "
        "} "
        ".normal-box { "
        "  width: 200px; "
        "  height: 100px; "
        "  white-space: normal; "
        "  overflow: hidden; "
        "  border: 2px solid #ff4444; "
        "  font-size: 14px; "
        "  color: #ffffff; "
        "  background-color: #2d2d2d; "
        "  padding: 5px; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    printf("White-space CSS Property Test\n");
    printf("==============================\n");
    printf("Testing:\n");
    printf("  1. white-space: nowrap - no word wrapping\n");
    printf("  2. white-space: pre - preserves newlines\n");
    printf("  3. white-space: normal - wraps at words\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765 ./whitespace_test\n");
    printf("\n");
    
    AppData initial_data = { .dummy = 0 };
    AzRefAny app_data = AppData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("white-space Test");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 500.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
