/**
 * Scrollbar Drag E2E Test
 * 
 * Tests scrollbar thumb dragging:
 * 1. Get scrollbar geometry via get_scrollbar_info
 * 2. MouseDown on scrollbar thumb
 * 3. MouseMove to drag
 * 4. MouseUp to release
 * 5. Click on track for page-scroll
 * 6. Click on up/down buttons for line-scroll
 * 
 * Creates a container with many items to ensure scrollbar is visible.
 * 
 * Compile:
 *   cd tests/e2e && cc scrollbar_drag.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o scrollbar_drag -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./scrollbar_drag
 * Test with: ./test_scrollbar_drag.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define NUM_ITEMS 30

typedef struct {
    int scroll_event_count;
    float last_scroll_y;
    int mouse_down_count;
    int mouse_up_count;
} ScrollbarDragData;

void ScrollbarDragData_destructor(void* data) {}

AzJson ScrollbarDragData_toJson(AzRefAny refany);
AzResultRefAnyString ScrollbarDragData_fromJson(AzJson json);

AZ_REFLECT_JSON(ScrollbarDragData, ScrollbarDragData_destructor, ScrollbarDragData_toJson, ScrollbarDragData_fromJson)

AzJson ScrollbarDragData_toJson(AzRefAny refany) {
    ScrollbarDragDataRef ref = ScrollbarDragDataRef_create(&refany);
    if (!ScrollbarDragData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    AzJsonKeyValue entries[4] = {
        AzJsonKeyValue_create(AZ_STR("scroll_event_count"), AzJson_int(ref.ptr->scroll_event_count)),
        AzJsonKeyValue_create(AZ_STR("last_scroll_y"), AzJson_float((double)ref.ptr->last_scroll_y)),
        AzJsonKeyValue_create(AZ_STR("mouse_down_count"), AzJson_int(ref.ptr->mouse_down_count)),
        AzJsonKeyValue_create(AZ_STR("mouse_up_count"), AzJson_int(ref.ptr->mouse_up_count))
    };
    
    ScrollbarDragDataRef_delete(&ref);
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 4);
    return AzJson_object(vec);
}

AzResultRefAnyString ScrollbarDragData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Create an item for the list
AzDom create_item(int index) {
    char buffer[64];
    int len = snprintf(buffer, sizeof(buffer), "Item %d - Scroll or drag to see more", index);
    
    AzString text = AzString_copyFromBytes((uint8_t*)buffer, 0, len);
    AzDom item = AzDom_createDiv();
    AzDom_addChild(&item, AzDom_createText(text));
    
    // Alternate colors
    const char* bg_color = (index % 2 == 0) ? "#3498db" : "#2980b9";
    char style[128];
    int style_len = snprintf(style, sizeof(style),
        "padding: 15px; margin: 4px 8px; background-color: %s; "
        "border-radius: 4px; color: white; font-size: 16px;",
        bg_color);
    
    AzString style_str = AzString_copyFromBytes((uint8_t*)style, 0, style_len);
    AzDom_setInlineStyle(&item, style_str);
    
    return item;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    ScrollbarDragDataRef ref = ScrollbarDragDataRef_create(&data);
    if (!ScrollbarDragData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Status bar
    char status[128];
    snprintf(status, sizeof(status),
             "Scroll Events: %d | Scroll Y: %.1f | Down: %d | Up: %d",
             ref.ptr->scroll_event_count, ref.ptr->last_scroll_y,
             ref.ptr->mouse_down_count, ref.ptr->mouse_up_count);
    
    ScrollbarDragDataRef_delete(&ref);
    
    // Create scroll container with items
    AzDom scroll_container = AzDom_createDiv();
    AzDom_addClass(&scroll_container, AZ_STR("scroll-container"));
    
    for (int i = 1; i <= NUM_ITEMS; i++) {
        AzDom item = create_item(i);
        AzDom_addChild(&scroll_container, item);
    }
    
    // Status bar
    AzDom status_bar = AzDom_createDiv();
    AzDom_addChild(&status_bar, AzDom_createText(AZ_STR(status)));
    AzDom_addClass(&status_bar, AZ_STR("status"));
    
    // Instructions
    AzDom instructions = AzDom_createDiv();
    AzDom_addChild(&instructions, AzDom_createText(AZ_STR(
        "Test scrollbar interaction:\n"
        "1. Wheel scroll on container\n"
        "2. Drag scrollbar thumb\n"
        "3. Click track for page scroll\n"
        "4. Click arrows for line scroll"
    )));
    AzDom_addClass(&instructions, AZ_STR("instructions"));
    
    // Build body
    AzDom body = AzDom_createBody();
    
    AzDom label = AzDom_createDiv();
    AzDom_addChild(&label, AzDom_createText(AZ_STR("Scrollbar Drag Test:")));
    AzDom_addClass(&label, AZ_STR("label"));
    AzDom_addChild(&body, label);
    
    AzDom_addChild(&body, scroll_container);
    AzDom_addChild(&body, status_bar);
    AzDom_addChild(&body, instructions);
    
    // CSS
    const char* css_str = 
        "body { "
        "  background-color: #2c3e50; "
        "  display: flex; "
        "  flex-direction: column; "
        "  padding: 20px; "
        "  flex-grow: 1; "
        "} "
        ".label { "
        "  font-size: 22px; "
        "  color: #ecf0f1; "
        "  margin-bottom: 15px; "
        "} "
        ".scroll-container { "
        "  width: 100%; "
        "  height: 250px; "
        "  overflow-y: auto; "
        "  overflow-x: hidden; "
        "  background-color: #34495e; "
        "  border: 2px solid #7f8c8d; "
        "  border-radius: 8px; "
        "} "
        ".status { "
        "  font-size: 14px; "
        "  color: #bdc3c7; "
        "  margin-top: 15px; "
        "  padding: 10px; "
        "  background-color: #1a252f; "
        "  border-radius: 4px; "
        "} "
        ".instructions { "
        "  font-size: 14px; "
        "  color: #95a5a6; "
        "  margin-top: 10px; "
        "  padding: 10px; "
        "  white-space: pre-wrap; "
        "  line-height: 1.6; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    printf("Scrollbar Drag E2E Test\n");
    printf("=======================\n");
    printf("Tests scrollbar thumb dragging:\n");
    printf("  1. get_scrollbar_info → scrollbar geometry\n");
    printf("  2. mouse_down on thumb\n");
    printf("  3. mouse_move to drag\n");
    printf("  4. mouse_up to release\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765 ./scrollbar_drag\n");
    printf("Test: ./test_scrollbar_drag.sh\n");
    printf("\n");
    
    ScrollbarDragData initial_data = {
        .scroll_event_count = 0,
        .last_scroll_y = 0.0f,
        .mouse_down_count = 0,
        .mouse_up_count = 0
    };
    
    AzRefAny app_data = ScrollbarDragData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Scrollbar Drag Test");
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
