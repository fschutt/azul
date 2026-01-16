/**
 * percent_width.c - E2E Test for CSS percentage width resolution
 * 
 * This example tests that percentage widths are correctly resolved
 * in the layout solver. The test creates:
 * 
 * 1. A container with explicit width (400px)
 * 2. Child divs with percentage widths (25%, 50%, 75%, 100%)
 * 
 * The layout solver should resolve these percentages against the
 * container's width, NOT against 0 or some incorrect value.
 * 
 * Expected layout:
 * - Container: 400px wide
 * - 25% child: 100px wide
 * - 50% child: 200px wide  
 * - 75% child: 300px wide
 * - 100% child: 400px wide
 */

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int dummy;
} PercentTestData;

void PercentTestData_destructor(void* data) {
    // Nothing to free
}

AZ_REFLECT(PercentTestData, PercentTestData_destructor);

AzStyledDom layout_percent_test(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data;
    (void)info;
    
    // Body with fixed width to test against
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes(
        "width: 400px; padding: 0px;", 0, 27
    );
    AzDom_setInlineStyle(&body, body_style);
    
    // Container: should be 400px wide (100% of body)
    AzDom container = AzDom_createDiv();
    AzString container_class = AzString_copyFromBytes("test-container", 0, 14);
    AzDom_addClass(&container, container_class);
    AzString container_style = AzString_copyFromBytes(
        "width: 100%; background-color: #eeeeee;", 0, 39
    );
    AzDom_setInlineStyle(&container, container_style);
    
    // Child 1: 25% width = should be 100px
    {
        AzDom child = AzDom_createDiv();
        AzString class_name = AzString_copyFromBytes("child-25", 0, 8);
        AzDom_addClass(&child, class_name);
        AzString style = AzString_copyFromBytes(
            "width: 25%; height: 20px; background-color: #4CAF50;", 0, 52
        );
        AzDom_setInlineStyle(&child, style);
        AzDom_addChild(&container, child);
    }
    
    // Child 2: 50% width = should be 200px
    {
        AzDom child = AzDom_createDiv();
        AzString class_name = AzString_copyFromBytes("child-50", 0, 8);
        AzDom_addClass(&child, class_name);
        AzString style = AzString_copyFromBytes(
            "width: 50%; height: 20px; background-color: #2196F3;", 0, 52
        );
        AzDom_setInlineStyle(&child, style);
        AzDom_addChild(&container, child);
    }
    
    // Child 3: 75% width = should be 300px
    {
        AzDom child = AzDom_createDiv();
        AzString class_name = AzString_copyFromBytes("child-75", 0, 8);
        AzDom_addClass(&child, class_name);
        AzString style = AzString_copyFromBytes(
            "width: 75%; height: 20px; background-color: #FF9800;", 0, 52
        );
        AzDom_setInlineStyle(&child, style);
        AzDom_addChild(&container, child);
    }
    
    // Child 4: 100% width = should be 400px
    {
        AzDom child = AzDom_createDiv();
        AzString class_name = AzString_copyFromBytes("child-100", 0, 9);
        AzDom_addClass(&child, class_name);
        AzString style = AzString_copyFromBytes(
            "width: 100%; height: 20px; background-color: #9C27B0;", 0, 53
        );
        AzDom_setInlineStyle(&child, style);
        AzDom_addChild(&container, child);
    }
    
    AzDom_addChild(&body, container);
    
    return AzDom_style(&body, AzCss_empty());
}

int main() {
    printf("Percentage Width Test\n");
    printf("=====================\n");
    printf("Testing CSS percentage width resolution\n");
    printf("\n");
    printf("Expected layout:\n");
    printf("  Container (100%% of 400px): 400px\n");
    printf("  Child 25%%: 100px\n");
    printf("  Child 50%%: 200px\n");
    printf("  Child 75%%: 300px\n");
    printf("  Child 100%%: 400px\n");
    printf("\n");
    
    PercentTestData model = { .dummy = 0 };
    AzRefAny data = PercentTestData_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_percent_test);
    AzString title = AzString_copyFromBytes("Percentage Width Test", 0, 21);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 200.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
