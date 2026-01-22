/**
 * Focus & Tab Navigation E2E Test
 * 
 * Simple grid of colored rectangles to test:
 * 1. Tab key navigation between focusable elements
 * 2. Shift+Tab for reverse navigation
 * 3. Enter/Space key activation (triggers click callback)
 * 4. Escape key to clear focus
 * 5. :focus CSS pseudo-class styling (color change on focus)
 * 
 * Run with: AZUL_DEBUG=8765 ./focus
 * Test with: curl -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "Tab"}'
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Helper macro
#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int click_count_button1;
    int click_count_button2;
    int click_count_button3;
    int last_clicked_button;
} FocusTestData;

void FocusTestData_destructor(void* data) {
    // Nothing to free
}

// Forward declarations for JSON serialization
AzJson FocusTestData_toJson(AzRefAny refany);
AzResultRefAnyString FocusTestData_fromJson(AzJson json);

// Register with JSON support for debug API serialization
AZ_REFLECT_JSON(FocusTestData, FocusTestData_destructor, FocusTestData_toJson, FocusTestData_fromJson)

// JSON Serialization - Convert FocusTestData to JSON
AzJson FocusTestData_toJson(AzRefAny refany) {
    FocusTestDataRef ref = FocusTestDataRef_create(&refany);
    if (!FocusTestData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    AzJsonKeyValue entries[4] = {
        AzJsonKeyValue_create(AZ_STR("click_count_button1"), AzJson_int(ref.ptr->click_count_button1)),
        AzJsonKeyValue_create(AZ_STR("click_count_button2"), AzJson_int(ref.ptr->click_count_button2)),
        AzJsonKeyValue_create(AZ_STR("click_count_button3"), AzJson_int(ref.ptr->click_count_button3)),
        AzJsonKeyValue_create(AZ_STR("last_clicked_button"), AzJson_int(ref.ptr->last_clicked_button))
    };
    
    FocusTestDataRef_delete(&ref);
    
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 4);
    return AzJson_object(vec);
}

// JSON Deserialization (not used in this test, but required by macro)
AzResultRefAnyString FocusTestData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Callback for Button 1 (red box)
AzUpdate on_button1_click(AzRefAny data, AzCallbackInfo info) {
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button1++;
        d.ptr->last_clicked_button = 1;
        fprintf(stderr, "Button 1 clicked! Total: %d\n", d.ptr->click_count_button1);
        fflush(stderr);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Callback for Button 2 (green box)
AzUpdate on_button2_click(AzRefAny data, AzCallbackInfo info) {
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button2++;
        d.ptr->last_clicked_button = 2;
        fprintf(stderr, "Button 2 clicked! Total: %d\n", d.ptr->click_count_button2);
        fflush(stderr);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Callback for Button 3 (blue box)
AzUpdate on_button3_click(AzRefAny data, AzCallbackInfo info) {
    FocusTestDataRefMut d = FocusTestDataRefMut_create(&data);
    if (FocusTestData_downcastMut(&data, &d)) {
        d.ptr->click_count_button3++;
        d.ptr->last_clicked_button = 3;
        fprintf(stderr, "Button 3 clicked! Total: %d\n", d.ptr->click_count_button3);
        fflush(stderr);
        FocusTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    FocusTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Create a focusable colored box
AzDom create_box(int button_num, AzCallbackType click_callback, AzRefAny data) {
    AzDom box = AzDom_createDiv();
    
    // Add click callback - use leftMouseUp for click
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_leftMouseUp());
    AzDom_addCallback(&box, event, AzRefAny_clone(&data), click_callback);
    
    // Make focusable with tabindex=0 (Auto)
    AzDom_setTabIndex(&box, AzTabIndex_auto());
    
    // Add classes for CSS styling - add each class separately!
    AzDom_addClass(&box, AZ_STR("box"));
    
    char class_name[32];
    snprintf(class_name, sizeof(class_name), "box-%d", button_num);
    AzDom_addClass(&box, AZ_STR(class_name));
    
    return box;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    FocusTestDataRef d = FocusTestDataRef_create(&data);
    if (!FocusTestData_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    FocusTestDataRef_delete(&d);
    
    // Create container for the 3 boxes
    AzDom container = AzDom_createDiv();
    AzDom_addClass(&container, AZ_STR("container"));
    
    // Create three colored boxes
    AzDom box1 = create_box(1, on_button1_click, data);  // Red
    AzDom box2 = create_box(2, on_button2_click, data);  // Green
    AzDom box3 = create_box(3, on_button3_click, data);  // Blue
    
    AzDom_addChild(&container, box1);
    AzDom_addChild(&container, box2);
    AzDom_addChild(&container, box3);
    
    // Build body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, container);
    
    // CSS with :focus pseudo-class for visual feedback
    // When focused, boxes get a bright yellow border and lighter color
    const char* css_str = 
        "body { "
        "  background-color: #2c3e50; "
        "  display: flex; "
        "  justify-content: center; "
        "  align-items: center; "
        "  flex-grow: 1; "
        "} "
        ".container { "
        "  display: flex; "
        "  flex-direction: row; "
        "  gap: 20px; "
        "} "
        ".box { "
        "  width: 100px; "
        "  height: 100px; "
        "  border: 4px solid transparent; "
        "  border-radius: 8px; "
        "  cursor: pointer; "
        "} "
        ".box:focus { "
        "  border-color: #f1c40f; "
        "} "
        ".box-1 { background-color: #e74c3c; } "
        ".box-1:focus { background-color: #ff6b6b; } "
        ".box-2 { background-color: #27ae60; } "
        ".box-2:focus { background-color: #2ecc71; } "
        ".box-3 { background-color: #3498db; } "
        ".box-3:focus { background-color: #5dade2; } ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    // Initialize app data
    FocusTestData initial_data = {
        .click_count_button1 = 0,
        .click_count_button2 = 0,
        .click_count_button3 = 0,
        .last_clicked_button = 0
    };
    
    AzRefAny app_data = FocusTestData_upcast(initial_data);
    
    // Create window options with layout callback
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Focus Test - Tab to navigate, Enter/Space to click");
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 300.0;
    
    // Create app config and app
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    
    // Run the app with the window
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
