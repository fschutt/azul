/**
 * Scrolling E2E Test
 * 
 * This example creates an overflowing body node to test:
 * 1. Automatic scrollbar display when content overflows
 * 2. Programmatic content scrolling via debug API
 * 3. Scroll position persistence across relayouts
 * 
 * Run with: AZUL_DEBUG=8765 ./scrolling
 * Test with: curl -X POST http://localhost:8765/event -d '{"type":"scroll","x":200,"y":200,"delta_x":0,"delta_y":-100}'
 */

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int item_count;
} ScrollTestData;

void ScrollTestData_destructor(void* data) {
    // Nothing to free
}

AZ_REFLECT(ScrollTestData, ScrollTestData_destructor);

// Generate a colored item for visibility
AzDom create_scroll_item(int index) {
    char buffer[128];
    int len = snprintf(buffer, sizeof(buffer), "Item %d - Scroll to see more content below", index);
    
    AzString text = AzString_copyFromBytes(buffer, 0, len);
    AzDom item = AzDom_createDiv();
    AzDom_addChild(&item, AzDom_createText(text));
    
    // Alternate background colors for visibility
    char style[256];
    const char* bg_color = (index % 2 == 0) ? "#e8e8e8" : "#f8f8f8";
    int style_len = snprintf(style, sizeof(style), 
        "padding: 20px; margin: 5px; background-color: %s; "
        "border: 1px solid #ccc; border-radius: 4px; font-size: 16px;",
        bg_color);
    
    AzString style_str = AzString_copyFromBytes(style, 0, style_len);
    AzDom_setInlineStyle(&item, style_str);
    
    return item;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    ScrollTestDataRef d = ScrollTestDataRef_create(&data);
    if (!ScrollTestData_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    
    int item_count = d.ptr->item_count;
    ScrollTestDataRef_delete(&d);
    
    // Create header
    AzString header_text = AzString_copyFromBytes("Scrolling Test - Overflowing Content", 0, 37);
    AzDom header = AzDom_createDiv();
    AzDom_addChild(&header, AzDom_createText(header_text));
    AzString header_style = AzString_copyFromBytes(
        "padding: 15px; background-color: #4a90d9; color: white; "
        "font-size: 24px; font-weight: bold; text-align: center;", 0, 121);
    AzDom_setInlineStyle(&header, header_style);
    
    // Create scrollable container with many items
    AzDom scroll_container = AzDom_createDiv();
    
    // Add many items to cause overflow
    for (int i = 1; i <= item_count; i++) {
        AzDom item = create_scroll_item(i);
        AzDom_addChild(&scroll_container, item);
    }
    
    // Style the container to have fixed height with overflow: auto
    // This should trigger automatic vertical scrollbar
    AzString container_style = AzString_copyFromBytes(
        "flex: 1; overflow: auto; padding: 10px; background-color: #ffffff; "
        "border: 2px solid #4a90d9; margin: 10px;", 0, 111);
    AzDom_setInlineStyle(&scroll_container, container_style);
    
    // Create footer with scroll info
    AzString footer_text = AzString_copyFromBytes(
        "Use mouse wheel or drag scrollbar to scroll. Debug API: POST scroll event.", 0, 75);
    AzDom footer = AzDom_createDiv();
    AzDom_addChild(&footer, AzDom_createText(footer_text));
    AzString footer_style = AzString_copyFromBytes(
        "padding: 10px; background-color: #f0f0f0; color: #666; "
        "font-size: 12px; text-align: center;", 0, 95);
    AzDom_setInlineStyle(&footer, footer_style);
    
    // Build body with flex column layout
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, header);
    AzDom_addChild(&body, scroll_container);
    AzDom_addChild(&body, footer);
    
    // Body style: full height flex column
    AzString body_style = AzString_copyFromBytes(
        "display: flex; flex-direction: column; height: 100%; box-sizing: border-box;", 0, 76);
    AzDom_setInlineStyle(&body, body_style);
    
    AzCss css = AzCss_empty();
    return AzDom_style(&body, css);
}

int main(int argc, char** argv) {
    // Parse command line for item count (default 50)
    int item_count = 50;
    if (argc > 1) {
        item_count = atoi(argv[1]);
        if (item_count < 1) item_count = 50;
    }
    
    printf("Scrolling Test\n");
    printf("==============\n");
    printf("Creating %d items to test scrolling\n", item_count);
    printf("\n");
    printf("To test with debug API:\n");
    printf("  AZUL_DEBUG=8765 ./scrolling\n");
    printf("\n");
    printf("Example commands:\n");
    printf("  # Get window state\n");
    printf("  curl -X POST http://localhost:8765/event -d '{\"type\":\"get_state\"}'\n");
    printf("\n");
    printf("  # Scroll down 100 pixels at position (200, 200)\n");
    printf("  curl -X POST http://localhost:8765/event -d '{\"type\":\"scroll\",\"x\":200,\"y\":200,\"delta_x\":0,\"delta_y\":-100}'\n");
    printf("\n");
    printf("  # Get DOM tree\n");
    printf("  curl -X POST http://localhost:8765/event -d '{\"type\":\"get_dom_tree\"}'\n");
    printf("\n");
    printf("  # Take native screenshot\n");
    printf("  curl -X POST http://localhost:8765/event -d '{\"type\":\"take_native_screenshot\"}'\n");
    printf("\n");
    
    ScrollTestData model = { .item_count = item_count };
    AzRefAny data = ScrollTestData_upcast(model);
    
    AzLayoutCallback layout_cb = AzLayoutCallback_create(layout);
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_cb);
    AzString title = AzString_copyFromBytes((const uint8_t*)"Scrolling Test", 0, 14);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
