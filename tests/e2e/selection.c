/**
 * Text Selection E2E Test
 * 
 * This example creates a window with 3 paragraphs:
 * 1. First paragraph - selectable text
 * 2. Second paragraph - user-select: none (NOT selectable)
 * 3. Third paragraph - selectable text
 * 
 * Used to test:
 * - Text selection across multiple paragraphs
 * - user-select: none CSS property is respected
 * - Selection state can be queried via debug API
 * 
 * Run with: AZUL_DEBUG=8765 ./selection
 * Test with: curl -X POST http://localhost:8765/ -d '{"op":"get_selection_state"}'
 */

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int click_count;
} SelectionTestData;

void SelectionTestData_destructor(void* data) {
    // Nothing to free
}

AZ_REFLECT(SelectionTestData, SelectionTestData_destructor);

// Click handler for paragraph 1
AzUpdate on_p1_click(AzRefAny data, AzCallbackInfo info) {
    printf("[CLICK] Paragraph 1 was clicked!\n");
    fflush(stdout);
    return AzUpdate_DoNothing;
}

// Click handler for paragraph 1's text node
AzUpdate on_p1_text_click(AzRefAny data, AzCallbackInfo info) {
    printf("[CLICK] Paragraph 1 TEXT NODE was clicked!\n");
    fflush(stdout);
    return AzUpdate_DoNothing;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Create header with instructions (compact)
    // Paragraph 1: Selectable
    AzString p1_text = AzString_copyFromBytes(
        "FIRST PARAGRAPH - This text is selectable. Start your selection here.", 0, 70);
    AzDom p1 = AzDom_createDiv();
    
    // Create text node with click handler
    AzDom p1_text_node = AzDom_createText(p1_text);
    AzEventFilter p1_text_event = AzEventFilter_hover(AzHoverEventFilter_mouseDown());
    AzDom_addCallback(&p1_text_node, p1_text_event, AzRefAny_clone(&data), on_p1_text_click);
    AzDom_addChild(&p1, p1_text_node);
    
    // Add click handler to the paragraph div
    AzEventFilter p1_event = AzEventFilter_hover(AzHoverEventFilter_mouseDown());
    AzDom_addCallback(&p1, p1_event, AzRefAny_clone(&data), on_p1_click);
    
    AzString p1_style = AzString_copyFromBytes(
        "font-size: 28px; padding: 15px; background-color: #c0ffc0; margin: 8px;", 0, 73);
    AzDom_setInlineStyle(&p1, p1_style);
    AzString p1_class = AzString_copyFromBytes("paragraph paragraph-1 selectable", 0, 32);
    AzDom_addClass(&p1, p1_class);
    
    // Paragraph 2: NOT selectable (user-select: none)
    AzString p2_text = AzString_copyFromBytes(
        "SECOND PARAGRAPH - user-select: none - This should be SKIPPED!", 0, 63);
    AzDom p2 = AzDom_createDiv();
    AzDom_addChild(&p2, AzDom_createText(p2_text));
    AzString p2_style = AzString_copyFromBytes(
        "font-size: 28px; padding: 15px; background-color: #ffc0c0; margin: 8px; user-select: none;", 0, 92);
    AzDom_setInlineStyle(&p2, p2_style);
    AzString p2_class = AzString_copyFromBytes("paragraph paragraph-2 non-selectable", 0, 36);
    AzDom_addClass(&p2, p2_class);
    
    // Paragraph 3: Selectable
    AzString p3_text = AzString_copyFromBytes(
        "THIRD PARAGRAPH - This text is also selectable. End your selection here.", 0, 73);
    AzDom p3 = AzDom_createDiv();
    AzDom_addChild(&p3, AzDom_createText(p3_text));
    AzString p3_style = AzString_copyFromBytes(
        "font-size: 28px; padding: 15px; background-color: #c0c0ff; margin: 8px;", 0, 73);
    AzDom_setInlineStyle(&p3, p3_style);
    AzString p3_class = AzString_copyFromBytes("paragraph paragraph-3 selectable", 0, 32);
    AzDom_addClass(&p3, p3_class);
    
    // Build body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, p1);
    AzDom_addChild(&body, p2);
    AzDom_addChild(&body, p3);
    
    AzString body_style = AzString_copyFromBytes(
        "display: flex; flex-direction: column; height: 100%; box-sizing: border-box;", 0, 76);
    AzDom_setInlineStyle(&body, body_style);
    
    AzCss css = AzCss_empty();
    return AzDom_style(&body, css);
}

int main(int argc, char** argv) {
    printf("Text Selection Test\n");
    printf("====================\n");
    printf("This test creates 3 paragraphs:\n");
    printf("  - Paragraph 1: Selectable (gray background)\n");
    printf("  - Paragraph 2: NOT selectable - user-select: none (red background)\n");
    printf("  - Paragraph 3: Selectable (green background)\n");
    printf("\n");
    printf("To test with debug API:\n");
    printf("  AZUL_DEBUG=8765 ./selection\n");
    printf("\n");
    printf("Example commands:\n");
    printf("  # Get selection state\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\":\"get_selection_state\"}'\n");
    printf("\n");
    printf("  # Get paragraph layout\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\":\"get_node_layout\",\"selector\":\".paragraph-1\"}'\n");
    printf("\n");
    
    SelectionTestData model = { .click_count = 0 };
    AzRefAny data = SelectionTestData_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString title = AzString_copyFromBytes("Text Selection Test", 0, 19);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
