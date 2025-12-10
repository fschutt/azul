/**
 * Simple C test - minimal test to isolate crash
 */

#include "../../target/codegen/azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STRING(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    uint32_t counter;
} TestData;

void test_data_destructor(void* ptr) {
    /* No-op destructor */
}

AzStyledDom simple_layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    fprintf(stderr, "[simple_layout] Start\n");
    
    /* Create a simple div */
    fprintf(stderr, "[simple_layout] Creating div\n");
    AzDom root = AzDom_div();
    
    /* Add simple inline style */
    fprintf(stderr, "[simple_layout] Setting inline style\n");
    AzString style = AZ_STRING("width: 100px; height: 100px; background-color: red;");
    fprintf(stderr, "[simple_layout] Calling AzDom_setInlineStyle\n");
    AzDom_setInlineStyle(&root, style);
    fprintf(stderr, "[simple_layout] Style set\n");
    
    /* Style and return */
    fprintf(stderr, "[simple_layout] Calling AzDom_style\n");
    AzCss css = AzCss_empty();
    AzStyledDom styled = AzDom_style(&root, css);
    fprintf(stderr, "[simple_layout] Done!\n");
    
    return styled;
}

int main() {
    printf("Simple C Test\n");
    
    TestData test_data = { .counter = 0 };
    
    AzGlVoidPtrConst ptr_wrapper = { .ptr = &test_data, .run_destructor = false };
    
    AzRefAny data = AzRefAny_newC(
        ptr_wrapper,
        sizeof(TestData),
        sizeof(TestData),  /* align */
        0,
        AZ_STRING("TestData"),
        test_data_destructor
    );
    
    AzAppConfig config = AzAppConfig_new(simple_layout);
    AzApp app = AzApp_new(data, config);
    AzApp_run(&app, AzWindowCreateOptions_new(simple_layout));
    
    return 0;
}
