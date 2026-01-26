/**
 * Text Area E2E Test (Multi-Line)
 * 
 * Multi-line text area to test:
 * 1. Multi-line text with Enter key
 * 2. Vertical cursor movement (Up/Down arrows)
 * 3. Scroll-into-view when cursor moves off-screen
 * 4. Ctrl+Home / Ctrl+End
 * 5. Page Up / Page Down
 * 6. Line-wrapping behavior
 * 
 * Uses large font (36px) with limited height to force scrolling.
 * 
 * Compile:
 *   cd tests/e2e && cc text_area.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o text_area -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./text_area
 * Test with: ./test_text_area.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define MAX_TEXT 4096

typedef struct {
    char text[MAX_TEXT];
    int cursor_line;
    int cursor_col;
    int total_lines;
    int key_count;
    int scroll_count;
} TextAreaData;

void TextAreaData_destructor(void* data) {}

AzJson TextAreaData_toJson(AzRefAny refany);
AzResultRefAnyString TextAreaData_fromJson(AzJson json);

AZ_REFLECT_JSON(TextAreaData, TextAreaData_destructor, TextAreaData_toJson, TextAreaData_fromJson)

AzJson TextAreaData_toJson(AzRefAny refany) {
    TextAreaDataRef ref = TextAreaDataRef_create(&refany);
    if (!TextAreaData_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    // Count lines
    int lines = 1;
    for (const char* p = ref.ptr->text; *p; p++) {
        if (*p == '\n') lines++;
    }
    
    AzJsonKeyValue entries[6] = {
        AzJsonKeyValue_create(AZ_STR("text"), AzJson_string(AZ_STR(ref.ptr->text))),
        AzJsonKeyValue_create(AZ_STR("cursor_line"), AzJson_int(ref.ptr->cursor_line)),
        AzJsonKeyValue_create(AZ_STR("cursor_col"), AzJson_int(ref.ptr->cursor_col)),
        AzJsonKeyValue_create(AZ_STR("total_lines"), AzJson_int(lines)),
        AzJsonKeyValue_create(AZ_STR("key_count"), AzJson_int(ref.ptr->key_count)),
        AzJsonKeyValue_create(AZ_STR("scroll_count"), AzJson_int(ref.ptr->scroll_count))
    };
    
    TextAreaDataRef_delete(&ref);
    AzJsonKeyValueVec vec = AzJsonKeyValueVec_copyFromArray(entries, 6);
    return AzJson_object(vec);
}

AzResultRefAnyString TextAreaData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("Not implemented"));
}

// Track key events
AzUpdate on_key_down(AzRefAny data, AzCallbackInfo info) {
    printf("[DEBUG] on_key_down CALLED!\n");
    fflush(stdout);
    
    TextAreaDataRefMut ref = TextAreaDataRefMut_create(&data);
    if (!TextAreaData_downcastMut(&data, &ref)) {
        printf("[DEBUG] on_key_down: downcast failed\n");
        return AzUpdate_DoNothing;
    }
    ref.ptr->key_count++;
    printf("[DEBUG] on_key_down: key_count now %d\n", ref.ptr->key_count);
    fflush(stdout);
    TextAreaDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

// Track scroll events
AzUpdate on_scroll(AzRefAny data, AzCallbackInfo info) {
    printf("[DEBUG] on_scroll CALLED!\n");
    fflush(stdout);
    
    TextAreaDataRefMut ref = TextAreaDataRefMut_create(&data);
    if (!TextAreaData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    ref.ptr->scroll_count++;
    printf("[DEBUG] on_scroll: scroll_count now %d\n", ref.ptr->scroll_count);
    fflush(stdout);
    TextAreaDataRefMut_delete(&ref);
    return AzUpdate_DoNothing;  // Don't refresh DOM on scroll
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    TextAreaDataRef ref = TextAreaDataRef_create(&data);
    if (!TextAreaData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Count lines for status
    int lines = 1;
    for (const char* p = ref.ptr->text; *p; p++) {
        if (*p == '\n') lines++;
    }
    
    // Create textarea container (scrollable)
    AzDom textarea = AzDom_createDiv();
    
    // Add text content
    AzDom text_node = AzDom_createText(AZ_STR(ref.ptr->text));
    AzDom_addChild(&textarea, text_node);
    
    // Make it focusable
    AzDom_setTabIndex(&textarea, AzTabIndex_auto());
    AzDom_addClass(&textarea, AZ_STR("textarea"));
    
    printf("[DEBUG] layout(): registering VirtualKeyDown callback on textarea node\n");
    fflush(stdout);
    
    // Add event handlers - NOTE: use the function call, not the enum value!
    AzEventFilter key_filter = AzEventFilter_focus(AzFocusEventFilter_virtualKeyDown());
    AzDom_addCallback(&textarea, key_filter, AzRefAny_clone(&data), on_key_down);
    
    AzEventFilter scroll_filter = AzEventFilter_window(AzWindowEventFilter_scroll());
    AzDom_addCallback(&textarea, scroll_filter, AzRefAny_clone(&data), on_scroll);
    
    // Status bar
    char status[256];
    snprintf(status, sizeof(status), 
             "Lines: %d | Cursor: L%d C%d | Keys: %d | Scrolls: %d",
             lines, ref.ptr->cursor_line, ref.ptr->cursor_col,
             ref.ptr->key_count, ref.ptr->scroll_count);
    
    AzDom status_bar = AzDom_createDiv();
    AzDom_addChild(&status_bar, AzDom_createText(AZ_STR(status)));
    AzDom_addClass(&status_bar, AZ_STR("status"));
    
    TextAreaDataRef_delete(&ref);
    
    // Build body
    AzDom body = AzDom_createBody();
    
    // Label
    AzDom label = AzDom_createDiv();
    AzDom_addChild(&label, AzDom_createText(AZ_STR("Multi-Line Text Area (scroll test):")));
    AzDom_addClass(&label, AZ_STR("label"));
    AzDom_addChild(&body, label);
    
    AzDom_addChild(&body, textarea);
    AzDom_addChild(&body, status_bar);
    
    // CSS with scrollable textarea
    const char* css_str = 
        "body { "
        "  background-color: #1e1e1e; "
        "  display: flex; "
        "  flex-direction: column; "
        "  padding: 30px; "
        "  flex-grow: 1; "
        "} "
        ".label { "
        "  font-size: 20px; "
        "  color: #cccccc; "
        "  margin-bottom: 15px; "
        "} "
        ".textarea { "
        "  font-size: 36px; "
        "  font-family: monospace; "
        "  padding: 15px; "
        "  background-color: #2d2d2d; "
        "  color: #ffffff; "
        "  border: 3px solid #555555; "
        "  min-width: 600px; "
        "  height: 200px; "  /* Limited height to force scrolling */
        "  overflow-y: auto; "
        "  overflow-x: auto; "
        "  white-space: pre; "  /* pre respects \n, pre-wrap not implemented yet */
        "  line-height: 1.4; "
        "  cursor: text; "
        "} "
        ".textarea:focus { "
        "  border-color: #0078d4; "
        "} "
        ".status { "
        "  font-size: 16px; "
        "  color: #888888; "
        "  margin-top: 15px; "
        "  padding: 10px; "
        "  background-color: #252525; "
        "} ";
    
    AzCss css = AzCss_fromString(AZ_STR(css_str));
    return AzDom_style(&body, css);
}

int main() {
    printf("Text Area E2E Test\n");
    printf("==================\n");
    printf("Multi-line textarea for testing:\n");
    printf("  - Enter key for new lines\n");
    printf("  - Up/Down arrows for line navigation\n");
    printf("  - Scroll-into-view when cursor off-screen\n");
    printf("  - Ctrl+Home / Ctrl+End\n");
    printf("  - Page Up / Page Down\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765 ./text_area\n");
    printf("Test: ./test_text_area.sh\n");
    printf("\n");
    
    // Initialize with multiple lines
    TextAreaData initial_data = {
        .cursor_line = 1,
        .cursor_col = 1,
        .total_lines = 15,
        .key_count = 0,
        .scroll_count = 0
    };
    
    // Create multi-line text that overflows
    strcpy(initial_data.text,
        "Line 1: This is the first line of text.\n"
        "Line 2: Second line here.\n"
        "Line 3: Third line with more content.\n"
        "Line 4: Fourth line.\n"
        "Line 5: Fifth line - getting longer now.\n"
        "Line 6: Sixth line.\n"
        "Line 7: Seventh line.\n"
        "Line 8: Eighth line - below visible area.\n"
        "Line 9: Ninth line.\n"
        "Line 10: Tenth line.\n"
        "Line 11: Eleventh line.\n"
        "Line 12: Twelfth line.\n"
        "Line 13: Thirteenth line.\n"
        "Line 14: Fourteenth line.\n"
        "Line 15: Last line - scroll to see this!"
    );
    
    AzRefAny app_data = TextAreaData_upcast(initial_data);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Text Area Test");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 450.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(app_data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
