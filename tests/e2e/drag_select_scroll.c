/**
 * Drag-Select-Scroll E2E Test
 * 
 * Tests the combined behavior of:
 * 1. Text selection via mouse drag
 * 2. Auto-scroll when dragging near container edge
 * 3. Selection extends during auto-scroll
 * 4. Drag out of window behavior
 * 
 * Creates a scrollable container with text content that extends
 * beyond the visible area to test auto-scroll behavior.
 * 
 * Compile:
 *   cd tests/e2e && cc drag_select_scroll.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o drag_select_scroll -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./drag_select_scroll
 * Test with: ./test_drag_select_scroll.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define NUM_PARAGRAPHS 20

typedef struct {
    int selection_start;
    int selection_end;
    int auto_scroll_triggers;
    float scroll_y;
    int drag_active;
    int mouse_events;
} DragSelectScrollData;

void DragSelectScrollData_destructor(void* data) {}

AzJson DragSelectScrollData_toJson(AzRefAny refany);
AzResultRefAnyString DragSelectScrollData_fromJson(AzJson json);

AZ_REFLECT_JSON(DragSelectScrollData, DragSelectScrollData_destructor, DragSelectScrollData_toJson, DragSelectScrollData_fromJson)

AzJson DragSelectScrollData_toJson(AzRefAny refany) {
    DragSelectScrollDataRef ref = DragSelectScrollDataRef_create(&refany);
    if (!DragSelectScrollData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    AzJsonKeyValue entries[6] = {
        AzJsonKeyValue_create(AZ_STR("selection_start"), AzJson_int(ref.ptr->selection_start)),
        AzJsonKeyValue_create(AZ_STR("selection_end"), AzJson_int(ref.ptr->selection_end)),
        AzJsonKeyValue_create(AZ_STR("auto_scroll_triggers"), AzJson_int(ref.ptr->auto_scroll_triggers)),
        AzJsonKeyValue_create(AZ_STR("scroll_y"), AzJson_float((double)ref.ptr->scroll_y)),
        AzJsonKeyValue_create(AZ_STR("drag_active"), AzJson_int(ref.ptr->drag_active)),
        AzJsonKeyValue_create(AZ_STR("mouse_events"), AzJson_int(ref.ptr->mouse_events))
    };
    
    DragSelectScrollDataRef_delete(&ref);
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 6);
    return AzJson_object(vec);
}

AzResultRefAnyString DragSelectScrollData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Create a paragraph with text
AzDom create_paragraph(int index) {
    char buffer[256];
    int len = snprintf(buffer, sizeof(buffer),
        "Paragraph %d: This is some sample text for testing drag-to-select with auto-scroll. "
        "Keep dragging down to trigger auto-scroll behavior when the mouse reaches the container edge.",
        index);
    
    AzString text = AzString_copyFromBytes((uint8_t*)buffer, 0, len);
    AzDom p = AzDom_createDiv();
    AzDom_addChild(&p, AzDom_createText(text));
    
    // Alternate colors
    const char* bg_color = (index % 2 == 0) ? "#e8f4f8" : "#f8f4e8";
    char style[256];
    int style_len = snprintf(style, sizeof(style),
        "padding: 15px; margin: 5px 10px; background-color: %s; "
        "border-radius: 4px; font-size: 16px; line-height: 1.6; "
        "user-select: text;",  /* Enable text selection */
        bg_color);
    
    AzString style_str = AzString_copyFromBytes((uint8_t*)style, 0, style_len);
    AzDom_setInlineStyle(&p, style_str);
    
    char class_name[32];
    snprintf(class_name, sizeof(class_name), "paragraph paragraph-%d", index);
    AzDom_addClass(&p, AZ_STR(class_name));
    
    return p;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    DragSelectScrollDataRef ref = DragSelectScrollDataRef_create(&data);
    if (!DragSelectScrollData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Status bar
    char status[256];
    snprintf(status, sizeof(status),
             "Selection: %d-%d | Auto-scrolls: %d | Scroll Y: %.1f | Drag: %s | Events: %d",
             ref.ptr->selection_start, ref.ptr->selection_end,
             ref.ptr->auto_scroll_triggers, ref.ptr->scroll_y,
             ref.ptr->drag_active ? "YES" : "NO", ref.ptr->mouse_events);
    
    DragSelectScrollDataRef_delete(&ref);
    
    // Create scrollable content container
    AzDom scroll_container = AzDom_createDiv();
    AzDom_addClass(&scroll_container, AZ_STR("scroll-container"));
    
    // Add many paragraphs
    for (int i = 1; i <= NUM_PARAGRAPHS; i++) {
        AzDom p = create_paragraph(i);
        AzDom_addChild(&scroll_container, p);
    }
    
    // Instructions
    AzDom instructions = AzDom_createDiv();
    AzDom_addChild(&instructions, AzDom_createText(AZ_STR(
        "Test drag-to-select with auto-scroll:\n"
        "1. Click and drag to select text\n"
        "2. Drag to bottom edge → should auto-scroll down\n"
        "3. Drag to top edge → should auto-scroll up\n"
        "4. Selection should extend during auto-scroll"
    )));
    AzDom_addClass(&instructions, AZ_STR("instructions"));
    
    // Status bar
    AzDom status_bar = AzDom_createDiv();
    AzDom_addChild(&status_bar, AzDom_createText(AZ_STR(status)));
    AzDom_addClass(&status_bar, AZ_STR("status"));
    
    // Build body
    AzDom body = AzDom_createBody();
    
    AzDom label = AzDom_createDiv();
    AzDom_addChild(&label, AzDom_createText(AZ_STR("Drag-Select-Scroll Test:")));
    AzDom_addClass(&label, AZ_STR("label"));
    AzDom_addChild(&body, label);
    
    AzDom_addChild(&body, scroll_container);
    AzDom_addChild(&body, instructions);
    AzDom_addChild(&body, status_bar);
    
    // CSS
    const char* css_str = 
        "body { "
        "  background-color: #f5f5f5; "
        "  display: flex; "
        "  flex-direction: column; "
        "  padding: 15px; "
        "  flex-grow: 1; "
        "} "
        ".label { "
        "  font-size: 20px; "
        "  color: #333333; "
        "  margin-bottom: 10px; "
        "  font-weight: bold; "
        "} "
        ".scroll-container { "
        "  width: 100%; "
        "  height: 300px; "
        "  overflow-y: auto; "
        "  overflow-x: hidden; "
        "  background-color: #ffffff; "
        "  border: 2px solid #cccccc; "
        "  border-radius: 8px; "
        "} "
        ".instructions { "
        "  font-size: 14px; "
        "  color: #666666; "
        "  margin-top: 10px; "
        "  padding: 10px; "
        "  white-space: pre-wrap; "
        "  line-height: 1.6; "
        "  background-color: #fffbe6; "
        "  border-radius: 4px; "
        "} "
        ".status { "
        "  font-size: 13px; "
        "  color: #888888; "
        "  margin-top: 8px; "
        "  padding: 8px; "
        "  background-color: #f0f0f0; "
        "  border-radius: 4px; "
        "  font-family: monospace; "
        "} "
        "/* Selection highlight styling */ "
        "::selection { "
        "  background-color: #3399ff; "
        "  color: white; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    printf("Drag-Select-Scroll E2E Test\n");
    printf("===========================\n");
    printf("Tests combined behavior:\n");
    printf("  1. Text selection via mouse drag\n");
    printf("  2. Auto-scroll when dragging near edge\n");
    printf("  3. Selection extends during auto-scroll\n");
    printf("  4. Drag out of window behavior\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765 ./drag_select_scroll\n");
    printf("Test: ./test_drag_select_scroll.sh\n");
    printf("\n");
    
    DragSelectScrollData initial_data = {
        .selection_start = -1,
        .selection_end = -1,
        .auto_scroll_triggers = 0,
        .scroll_y = 0.0f,
        .drag_active = 0,
        .mouse_events = 0
    };
    
    AzRefAny app_data = DragSelectScrollData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Drag-Select-Scroll Test");
    window.window_state.size.dimensions.width = 700.0;
    window.window_state.size.dimensions.height = 550.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
