// Regular Scroll Container test - C
//
// Tests a normal overflow:auto scroll container with many child elements.
// This uses NO IFrame — all rows are real DOM children.
// Used to compare scroll behavior against the IFrame-based infinity.c
//
// Build: cc -o scrolling scrolling.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
// Run:   DYLD_LIBRARY_PATH=../../target/release ./scrolling

#include "azul.h"
#include <stdio.h>
#include <string.h>

#define TOTAL_ROWS 500
#define ROW_HEIGHT 30.0f

typedef struct {
    int total_rows;
} ScrollData;

void ScrollData_destructor(void* d) { }
AZ_REFLECT(ScrollData, ScrollData_destructor);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    ScrollDataRef d = ScrollDataRef_create(&data);
    int total = TOTAL_ROWS;
    if (ScrollData_downcastRef(&data, &d)) {
        total = d.ptr->total_rows;
        ScrollDataRef_delete(&d);
    }

    // Title
    AzString title_text = AzString_copyFromBytes(
        (const uint8_t*)"Regular Scroll Test (no IFrame)", 0, 31);
    AzDom title = AzDom_createDiv();
    AzDom_addChild(&title, AzDom_createText(title_text));
    AzString title_style = AzString_copyFromBytes(
        (const uint8_t*)"padding: 12px; background: #4a90d9; color: white; font-size: 18px; font-weight: bold;",
        0, 85);
    AzDom_setInlineStyle(&title, title_style);

    // Scroll container with many rows
    AzDom container = AzDom_createDiv();
    for (int i = 0; i < total; i++) {
        char buf[64];
        int len = snprintf(buf, sizeof(buf), "Row %d", i);
        AzString label = AzString_copyFromBytes((const uint8_t*)buf, 0, (size_t)len);
        AzDom text_node = AzDom_createText(label);

        AzDom row = AzDom_createDiv();
        AzDom_addChild(&row, text_node);

        char style[192];
        const char* bg = (i % 2 == 0) ? "#e8e8e8" : "#ffffff";
        int slen = snprintf(style, sizeof(style),
            "height: %.0fpx; min-height: %.0fpx; flex-shrink: 0; line-height: %.0fpx; padding-left: 8px; color: #000000; background: %s;",
            ROW_HEIGHT, ROW_HEIGHT, ROW_HEIGHT, bg);
        AzString style_str = AzString_copyFromBytes((const uint8_t*)style, 0, (size_t)slen);
        AzDom_setInlineStyle(&row, style_str);

        AzDom_addChild(&container, row);
    }

    AzString container_style = AzString_copyFromBytes(
        (const uint8_t*)"display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; overflow: auto; background: #ffff00; border: 10px solid #00ff00; margin: 8px; min-height: 0;",
        0, 162);
    AzDom_setInlineStyle(&container, container_style);

    // Footer
    char footer_buf[128];
    int flen = snprintf(footer_buf, sizeof(footer_buf),
        "Regular scroll container with %d real DOM rows (no IFrame).", total);
    AzString footer_text = AzString_copyFromBytes((const uint8_t*)footer_buf, 0, (size_t)flen);
    AzDom footer = AzDom_createDiv();
    AzDom_addChild(&footer, AzDom_createText(footer_text));
    AzString footer_style = AzString_copyFromBytes(
        (const uint8_t*)"padding: 8px; background: #f0f0f0; color: #666; font-size: 12px; text-align: center; flex-shrink: 0;",
        0, 100);
    AzDom_setInlineStyle(&footer, footer_style);

    // Body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, container);
    AzDom_addChild(&body, footer);
    AzString body_style = AzString_copyFromBytes(
        (const uint8_t*)"display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0;",
        0, 75);
    AzDom_setInlineStyle(&body, body_style);

    return AzDom_style(&body, AzCss_empty());
}

int main(void) {
    printf("Regular Scroll Test\n");
    printf("====================\n");
    printf("Rows: %d (real DOM children)\n\n", TOTAL_ROWS);

    ScrollData model = { .total_rows = TOTAL_ROWS };
    AzRefAny data = ScrollData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AzString_copyFromBytes(
        (const uint8_t*)"Regular Scroll - 500 rows", 0, 25);
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
