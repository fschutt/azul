/**
 * Focus & Scroll-Into-View E2E Test
 * 
 * Tests that tabbing to off-screen elements triggers automatic scrolling:
 * 1. Scroll container with many focusable items
 * 2. Tab to element that's below visible area → should scroll down
 * 3. Shift+Tab to element above visible area → should scroll up
 * 4. Focus set programmatically should also scroll into view
 * 
 * This is preparation for cursor movement and text selection.
 * 
 * DOM Structure:
 * ┌────────────────────────────────────────┐
 * │ scroll-container (overflow: auto)      │
 * │ ┌────────────────────────────────────┐ │
 * │ │ item-1 (visible)                   │ │
 * │ │ item-2 (visible)                   │ │
 * │ │ item-3 (visible)                   │ │
 * │ │ item-4 (partially visible)         │ │
 * │ ├────────────────────────────────────┤ │ ← scroll boundary
 * │ │ item-5 (off-screen)                │ │
 * │ │ item-6 (off-screen)                │ │
 * │ │ ...                                │ │
 * │ │ item-20 (off-screen)               │ │
 * │ └────────────────────────────────────┘ │
 * └────────────────────────────────────────┘
 * 
 * Run with: AZUL_DEBUG=8765 ./focus_scroll
 * Test with: ./test_scroll_into_view.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define NUM_ITEMS 20

typedef struct {
    int last_focused_item;
    int focus_count;
    float scroll_position;  // Approximate scroll position for testing
} ScrollTestData;

void ScrollTestData_destructor(void* data) {}

AzJson ScrollTestData_toJson(AzRefAny refany);
AzResultRefAnyString ScrollTestData_fromJson(AzJson json);

AZ_REFLECT_JSON(ScrollTestData, ScrollTestData_destructor, ScrollTestData_toJson, ScrollTestData_fromJson)

AzJson ScrollTestData_toJson(AzRefAny refany) {
    ScrollTestDataRef ref = ScrollTestDataRef_create(&refany);
    if (!ScrollTestData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    AzJsonKeyValue entries[3] = {
        AzJsonKeyValue_create(AZ_STR("last_focused_item"), AzJson_int(ref.ptr->last_focused_item)),
        AzJsonKeyValue_create(AZ_STR("focus_count"), AzJson_int(ref.ptr->focus_count)),
        AzJsonKeyValue_create(AZ_STR("scroll_position"), AzJson_float((double)ref.ptr->scroll_position))
    };
    
    ScrollTestDataRef_delete(&ref);
    
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 3);
    return AzJson_object(vec);
}

AzResultRefAnyString ScrollTestData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Generic focus callback
AzUpdate on_item_focus(AzRefAny data, AzCallbackInfo info, int item_num) {
    ScrollTestDataRefMut d = ScrollTestDataRefMut_create(&data);
    if (ScrollTestData_downcastMut(&data, &d)) {
        d.ptr->last_focused_item = item_num;
        d.ptr->focus_count++;
        fprintf(stderr, "Item %d focused! Total focus events: %d\n", item_num, d.ptr->focus_count);
        fflush(stderr);
        ScrollTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    ScrollTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Create focus callbacks for items 1-20
#define FOCUS_CB(n) \
    AzUpdate on_item##n##_focus(AzRefAny data, AzCallbackInfo info) { \
        return on_item_focus(data, info, n); \
    }

FOCUS_CB(1)  FOCUS_CB(2)  FOCUS_CB(3)  FOCUS_CB(4)  FOCUS_CB(5)
FOCUS_CB(6)  FOCUS_CB(7)  FOCUS_CB(8)  FOCUS_CB(9)  FOCUS_CB(10)
FOCUS_CB(11) FOCUS_CB(12) FOCUS_CB(13) FOCUS_CB(14) FOCUS_CB(15)
FOCUS_CB(16) FOCUS_CB(17) FOCUS_CB(18) FOCUS_CB(19) FOCUS_CB(20)

// Array of focus callbacks
typedef AzUpdate (*FocusCallback)(AzRefAny, AzCallbackInfo);
FocusCallback focus_callbacks[NUM_ITEMS] = {
    on_item1_focus, on_item2_focus, on_item3_focus, on_item4_focus, on_item5_focus,
    on_item6_focus, on_item7_focus, on_item8_focus, on_item9_focus, on_item10_focus,
    on_item11_focus, on_item12_focus, on_item13_focus, on_item14_focus, on_item15_focus,
    on_item16_focus, on_item17_focus, on_item18_focus, on_item19_focus, on_item20_focus
};

// Create a focusable list item
AzDom create_item(int item_num, AzRefAny data) {
    AzDom item = AzDom_createDiv();
    
    // Add focus callback
    AzEventFilter event = AzEventFilter_focus(AzFocusEventFilter_focusReceived());
    AzDom_addCallback(&item, event, AzRefAny_clone(&data), focus_callbacks[item_num - 1]);
    
    // Make focusable
    AzDom_setTabIndex(&item, AzTabIndex_auto());
    
    // Add classes
    AzDom_addClass(&item, AZ_STR("item"));
    
    char class_name[32];
    snprintf(class_name, sizeof(class_name), "item-%d", item_num);
    AzDom_addClass(&item, AZ_STR(class_name));
    
    // Add text label
    char label[64];
    snprintf(label, sizeof(label), "Item %d - Focusable Element", item_num);
    AzDom text = AzDom_createText(AZ_STR(label));
    AzDom_addChild(&item, text);
    
    return item;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    ScrollTestDataRef d = ScrollTestDataRef_create(&data);
    if (!ScrollTestData_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    ScrollTestDataRef_delete(&d);
    
    // Create scroll container
    AzDom scroll_container = AzDom_createDiv();
    AzDom_addClass(&scroll_container, AZ_STR("scroll-container"));
    
    // Create 20 focusable items
    for (int i = 1; i <= NUM_ITEMS; i++) {
        AzDom item = create_item(i, data);
        AzDom_addChild(&scroll_container, item);
    }
    
    // Build body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, scroll_container);
    
    // CSS with scroll container
    const char* css_str = 
        "body { "
        "  background-color: #2c3e50; "
        "  display: flex; "
        "  justify-content: center; "
        "  align-items: center; "
        "  flex-grow: 1; "
        "  padding: 40px; "
        "} "
        ".scroll-container { "
        "  width: 400px; "
        "  height: 250px; "  /* Only shows ~4 items */
        "  overflow-y: auto; "
        "  overflow-x: hidden; "
        "  background-color: #34495e; "
        "  border-radius: 12px; "
        "  border: 2px solid #7f8c8d; "
        "} "
        ".item { "
        "  height: 50px; "
        "  padding: 10px 20px; "
        "  margin: 5px 10px; "
        "  background-color: #3498db; "
        "  border: 3px solid transparent; "
        "  border-radius: 8px; "
        "  cursor: pointer; "
        "  display: flex; "
        "  align-items: center; "
        "  color: white; "
        "  font-size: 14px; "
        "} "
        ".item:focus { "
        "  border-color: #f1c40f; "
        "  background-color: #2980b9; "
        "} "
        ".item:hover { "
        "  background-color: #5dade2; "
        "} "
        /* Alternating colors for visual clarity */
        ".item-1, .item-3, .item-5, .item-7, .item-9, "
        ".item-11, .item-13, .item-15, .item-17, .item-19 { "
        "  background-color: #27ae60; "
        "} "
        ".item-1:focus, .item-3:focus, .item-5:focus, .item-7:focus, .item-9:focus, "
        ".item-11:focus, .item-13:focus, .item-15:focus, .item-17:focus, .item-19:focus { "
        "  background-color: #1e8449; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    ScrollTestData initial_data = {
        .last_focused_item = 0,
        .focus_count = 0,
        .scroll_position = 0.0f
    };
    
    AzRefAny app_data = ScrollTestData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Scroll Into View Test - Tab through items");
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 400.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
