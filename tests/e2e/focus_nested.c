/**
 * Focus & Tab Navigation E2E Test - Nested DOM Structures
 * 
 * Tests tab navigation behavior with nested DOM elements:
 * 1. Focusable elements inside non-focusable containers
 * 2. Nested focusable elements (parent and child both focusable)
 * 3. Tab order with mixed nesting depths
 * 4. Skip non-focusable intermediate nodes
 * 
 * DOM Structure:
 * ┌─────────────────────────────────────────────────────────────┐
 * │ body                                                         │
 * │ ┌─────────────────────────────────────────────────────────┐ │
 * │ │ container (not focusable)                               │ │
 * │ │ ┌─────────┐ ┌─────────────────────────────────────────┐ │ │
 * │ │ │ box-1   │ │ group-a (not focusable)                 │ │ │
 * │ │ │ (focus) │ │ ┌─────────┐ ┌─────────┐ ┌─────────┐    │ │ │
 * │ │ │ tabidx=1│ │ │ box-2   │ │ box-3   │ │ box-4   │    │ │ │
 * │ │ │         │ │ │ (focus) │ │ (focus) │ │ (focus) │    │ │ │
 * │ │ └─────────┘ │ │ tabidx=2│ │ tabidx=3│ │ tabidx=4│    │ │ │
 * │ │             │ └─────────┘ └─────────┘ └─────────┘    │ │ │
 * │ │             └─────────────────────────────────────────┘ │ │
 * │ │ ┌─────────────────────────────────────────────────────┐ │ │
 * │ │ │ group-b (FOCUSABLE - tabidx=5)                      │ │ │
 * │ │ │ ┌─────────┐ ┌─────────┐                             │ │ │
 * │ │ │ │ box-5   │ │ box-6   │                             │ │ │
 * │ │ │ │ (focus) │ │ (focus) │                             │ │ │
 * │ │ │ │ tabidx=6│ │ tabidx=7│                             │ │ │
 * │ │ │ └─────────┘ └─────────┘                             │ │ │
 * │ │ └─────────────────────────────────────────────────────┘ │ │
 * │ └─────────────────────────────────────────────────────────┘ │
 * └─────────────────────────────────────────────────────────────┘
 * 
 * Expected Tab Order: 1 → 2 → 3 → 4 → 5 (group-b) → 6 → 7 → wrap to 1
 * 
 * Run with: AZUL_DEBUG=8765 ./focus_nested
 * Test with: ./test_nested_tabs.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int last_focused_box;
    int focus_order[20];  // Track order of focus changes
    int focus_count;
} NestedTestData;

void NestedTestData_destructor(void* data) {
    // No-op: upcast copies the struct, so no manual memory management needed
}

AzJson NestedTestData_toJson(AzRefAny refany);
AzResultRefAnyString NestedTestData_fromJson(AzJson json);

AZ_REFLECT_JSON(NestedTestData, NestedTestData_destructor, NestedTestData_toJson, NestedTestData_fromJson)

AzJson NestedTestData_toJson(AzRefAny refany) {
    NestedTestDataRef ref = NestedTestDataRef_create(&refany);
    if (!NestedTestData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    // Build focus_order array - create temporary array then copy
    AzJson order_arr[20];
    int count = 0;
    for (int i = 0; i < ref.ptr->focus_count && i < 20; i++) {
        order_arr[count++] = AzJson_int(ref.ptr->focus_order[i]);
    }
    AzJsonVec order_vec = AzJsonVec_copyFromArray(order_arr, count);
    
    AzJsonKeyValue entries[3] = {
        AzJsonKeyValue_create(AZ_STR("last_focused_box"), AzJson_int(ref.ptr->last_focused_box)),
        AzJsonKeyValue_create(AZ_STR("focus_count"), AzJson_int(ref.ptr->focus_count)),
        AzJsonKeyValue_create(AZ_STR("focus_order"), AzJson_array(order_vec))
    };
    
    NestedTestDataRef_delete(&ref);
    
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 3);
    return AzJson_object(vec);
}

AzResultRefAnyString NestedTestData_fromJson(AzJson json) {
    // Parse values into local variables first
    int last_focused_box = 0;
    int focus_count = 0;
    int focus_order[20] = {0};
    
    // Extract values using the proper AzJson API
    if (AzJson_isObject(&json)) {
        // Get focus_count
        AzOptionJson focus_count_opt = AzJson_getKey(&json, AZ_STR("focus_count"));
        if (focus_count_opt.Some.tag == AzOptionJson_Tag_Some) {
            AzOptionI64 val = AzJson_asInt(&focus_count_opt.Some.payload);
            if (val.Some.tag == AzOptionI64_Tag_Some) {
                focus_count = (int)val.Some.payload;
            }
        }
        
        // Get last_focused_box
        AzOptionJson last_focused_opt = AzJson_getKey(&json, AZ_STR("last_focused_box"));
        if (last_focused_opt.Some.tag == AzOptionJson_Tag_Some) {
            AzOptionI64 val = AzJson_asInt(&last_focused_opt.Some.payload);
            if (val.Some.tag == AzOptionI64_Tag_Some) {
                last_focused_box = (int)val.Some.payload;
            }
        }
        
        // Get focus_order array
        AzOptionJson focus_order_opt = AzJson_getKey(&json, AZ_STR("focus_order"));
        if (focus_order_opt.Some.tag == AzOptionJson_Tag_Some) {
            AzJson* arr = &focus_order_opt.Some.payload;
            if (AzJson_isArray(arr)) {
                size_t len = AzJson_len(arr);
                for (size_t j = 0; j < len && j < 20; j++) {
                    AzOptionJson item_opt = AzJson_getIndex(arr, j);
                    if (item_opt.Some.tag == AzOptionJson_Tag_Some) {
                        AzOptionI64 val = AzJson_asInt(&item_opt.Some.payload);
                        if (val.Some.tag == AzOptionI64_Tag_Some) {
                            focus_order[j] = (int)val.Some.payload;
                        }
                    }
                }
            }
        }
    }
    
    // Create struct on stack and use upcast (which copies)
    NestedTestData data = {
        .last_focused_box = last_focused_box,
        .focus_count = focus_count
    };
    memcpy(data.focus_order, focus_order, sizeof(focus_order));
    
    AzRefAny refany = NestedTestData_upcast(data);
    return AzResultRefAnyString_ok(refany);
}

// Focus callback to track focus changes
AzUpdate on_focus_received(AzRefAny data, AzCallbackInfo info, int box_num) {
    // The 'data' parameter is a clone of the RefAny that was registered with the callback.
    // Since RefAny uses reference counting, this clone points to the SAME underlying data
    // as the original app state.
    NestedTestDataRefMut d = NestedTestDataRefMut_create(&data);
    if (NestedTestData_downcastMut(&data, &d)) {
        d.ptr->last_focused_box = box_num;
        if (d.ptr->focus_count < 20) {
            d.ptr->focus_order[d.ptr->focus_count++] = box_num;
        }
        fprintf(stderr, "Box %d focused! Order index: %d, focus_count now: %d\n", 
                box_num, d.ptr->focus_count, d.ptr->focus_count);
        fflush(stderr);
        NestedTestDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    NestedTestDataRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

// Macros to create focus callbacks for each box
#define FOCUS_CALLBACK(n) \
    AzUpdate on_box##n##_focus(AzRefAny data, AzCallbackInfo info) { \
        return on_focus_received(data, info, n); \
    }

FOCUS_CALLBACK(1)
FOCUS_CALLBACK(2)
FOCUS_CALLBACK(3)
FOCUS_CALLBACK(4)
FOCUS_CALLBACK(5)
FOCUS_CALLBACK(6)
FOCUS_CALLBACK(7)

// Create a focusable box with specific tab index
AzDom create_focusable_box(int box_num, int tab_index, AzCallbackType focus_callback, AzRefAny data) {
    AzDom box = AzDom_createDiv();
    
    // Add focus-in callback
    AzEventFilter event = AzEventFilter_focus(AzFocusEventFilter_focusReceived());
    AzDom_addCallback(&box, event, AzRefAny_clone(&data), focus_callback);
    
    // Set tab index
    if (tab_index > 0) {
        AzDom_setTabIndex(&box, AzTabIndex_overrideInParent((uint32_t)tab_index));
    } else {
        AzDom_setTabIndex(&box, AzTabIndex_auto());
    }
    
    // Add classes
    AzDom_addClass(&box, AZ_STR("box"));
    
    char class_name[32];
    snprintf(class_name, sizeof(class_name), "box-%d", box_num);
    AzDom_addClass(&box, AZ_STR(class_name));
    
    return box;
}

// Create a non-focusable group container
AzDom create_group(const char* class_name) {
    AzDom group = AzDom_createDiv();
    AzDom_addClass(&group, AZ_STR("group"));
    AzDom_addClass(&group, AZ_STR(class_name));
    return group;
}

// Create a focusable group container
AzDom create_focusable_group(const char* class_name, int tab_index, AzCallbackType focus_callback, AzRefAny data) {
    AzDom group = AzDom_createDiv();
    AzDom_addClass(&group, AZ_STR("group"));
    AzDom_addClass(&group, AZ_STR("focusable-group"));
    AzDom_addClass(&group, AZ_STR(class_name));
    
    // Add focus callback
    AzEventFilter event = AzEventFilter_focus(AzFocusEventFilter_focusReceived());
    AzDom_addCallback(&group, event, AzRefAny_clone(&data), focus_callback);
    
    // Set tab index
    AzDom_setTabIndex(&group, AzTabIndex_overrideInParent((uint32_t)tab_index));
    
    return group;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    NestedTestDataRef d = NestedTestDataRef_create(&data);
    if (!NestedTestData_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    NestedTestDataRef_delete(&d);
    
    // Build the nested structure
    AzDom container = AzDom_createDiv();
    AzDom_addClass(&container, AZ_STR("container"));
    
    // Box 1: standalone focusable box
    AzDom box1 = create_focusable_box(1, 1, on_box1_focus, data);
    AzDom_addChild(&container, box1);
    
    // Group A: non-focusable container with 3 focusable boxes
    AzDom group_a = create_group("group-a");
    AzDom box2 = create_focusable_box(2, 2, on_box2_focus, data);
    AzDom box3 = create_focusable_box(3, 3, on_box3_focus, data);
    AzDom box4 = create_focusable_box(4, 4, on_box4_focus, data);
    AzDom_addChild(&group_a, box2);
    AzDom_addChild(&group_a, box3);
    AzDom_addChild(&group_a, box4);
    AzDom_addChild(&container, group_a);
    
    // Group B: FOCUSABLE container with 2 focusable children
    // This tests parent-child focus relationship
    AzDom group_b = create_focusable_group("group-b", 5, on_box5_focus, data);
    AzDom box6 = create_focusable_box(6, 6, on_box6_focus, data);
    AzDom box7 = create_focusable_box(7, 7, on_box7_focus, data);
    AzDom_addChild(&group_b, box6);
    AzDom_addChild(&group_b, box7);
    AzDom_addChild(&container, group_b);
    
    // Build body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, container);
    
    // CSS
    const char* css_str = 
        "body { "
        "  background-color: #1a1a2e; "
        "  display: flex; "
        "  justify-content: center; "
        "  align-items: center; "
        "  flex-grow: 1; "
        "  padding: 20px; "
        "} "
        ".container { "
        "  display: flex; "
        "  flex-direction: column; "
        "  gap: 20px; "
        "  padding: 20px; "
        "  background-color: #16213e; "
        "  border-radius: 12px; "
        "} "
        ".group { "
        "  display: flex; "
        "  flex-direction: row; "
        "  gap: 15px; "
        "  padding: 15px; "
        "  background-color: #0f3460; "
        "  border-radius: 8px; "
        "  border: 2px solid transparent; "
        "} "
        ".focusable-group:focus { "
        "  border-color: #e94560; "
        "  background-color: #1a4a70; "
        "} "
        ".box { "
        "  width: 80px; "
        "  height: 80px; "
        "  border: 3px solid transparent; "
        "  border-radius: 6px; "
        "  cursor: pointer; "
        "  display: flex; "
        "  justify-content: center; "
        "  align-items: center; "
        "} "
        ".box:focus { "
        "  border-color: #f1c40f; "
        "} "
        /* Individual box colors */
        ".box-1 { background-color: #e74c3c; } "
        ".box-1:focus { background-color: #ff6b6b; } "
        ".box-2 { background-color: #e67e22; } "
        ".box-2:focus { background-color: #f39c12; } "
        ".box-3 { background-color: #f1c40f; } "
        ".box-3:focus { background-color: #f7dc6f; } "
        ".box-4 { background-color: #27ae60; } "
        ".box-4:focus { background-color: #2ecc71; } "
        ".box-6 { background-color: #3498db; } "
        ".box-6:focus { background-color: #5dade2; } "
        ".box-7 { background-color: #9b59b6; } "
        ".box-7:focus { background-color: #bb8fce; } ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    NestedTestData initial_data = {
        .last_focused_box = 0,
        .focus_count = 0
    };
    memset(initial_data.focus_order, 0, sizeof(initial_data.focus_order));
    
    AzRefAny app_data = NestedTestData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Nested Focus Test - Tab through nested elements");
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 400.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
