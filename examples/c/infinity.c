// Infinite Scrolling (VirtualizedView) - C
//
// Tests VirtualizedViewCallback with 4 million virtual rows, rendering only ~100 at a time.
// Scroll the yellow container and watch the virtualized view re-render the visible chunk.
//
// Build: cc -o infinity infinity.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
// Run:   DYLD_LIBRARY_PATH=../../target/release ./infinity

#include "azul.h"
#include <stdio.h>
#include <string.h>

#define TOTAL_ROWS    4000000
#define ROW_HEIGHT    30.0f
#define VISIBLE_ROWS  100

typedef struct {
    int total_rows;
} InfinityData;

void InfinityData_destructor(void* d) { }
AZ_REFLECT(InfinityData, InfinityData_destructor);

// ---------------------------------------------------------------------------
// VirtualizedView callback: renders only the visible chunk of rows
// ---------------------------------------------------------------------------
AzVirtualizedViewCallbackReturn render_rows(AzRefAny data, AzVirtualizedViewCallbackInfo info) {

    InfinityDataRef d = InfinityDataRef_create(&data);
    if (!InfinityData_downcastRef(&data, &d)) {
        return AzVirtualizedViewCallbackReturn_withDom(
            AzStyledDom_default(),
            AzLogicalSize_zero(), AzLogicalPosition_zero(),
            AzLogicalSize_zero(), AzLogicalPosition_zero()
        );
    }

    int total = d.ptr->total_rows;
    InfinityDataRef_delete(&d);

    // Current scroll position (positive downward)
    float scroll_y = info.scroll_offset.y;
    if (scroll_y < 0.0f) scroll_y = 0.0f;

    // Which row is at the top of the viewport?
    int first_row = (int)(scroll_y / ROW_HEIGHT);
    if (first_row < 0) first_row = 0;
    if (first_row >= total) first_row = total - 1;

    int count = VISIBLE_ROWS;
    if (first_row + count > total) count = total - first_row;

    // Build a simple column of rows
    AzDom container = AzDom_createDiv();

    for (int i = 0; i < count; i++) {
        int row_idx = first_row + i;

        // Label
        char buf[64];
        int len = snprintf(buf, sizeof(buf), "Row %d", row_idx);
        AzString label = AzString_copyFromBytes((const uint8_t*)buf, 0, (size_t)len);
        AzDom text_node = AzDom_createText(label);

        // Row div
        AzDom row = AzDom_createDiv();
        AzDom_addChild(&row, text_node);

        // Alternating colours, fixed height
        char style[128];
        const char* bg = (row_idx % 2 == 0) ? "#e8e8e8" : "#ffffff";
        int slen = snprintf(style, sizeof(style),
            "height: %.0fpx; line-height: %.0fpx; padding-left: 8px; background: %s;",
            ROW_HEIGHT, ROW_HEIGHT, bg);
        AzString style_str = AzString_copyFromBytes((const uint8_t*)style, 0, (size_t)slen);
        AzDom_setInlineStyle(&row, style_str);

        AzDom_addChild(&container, row);
    }

    AzStyledDom dom = AzDom_style(&container, AzCss_empty());

    // --- sizes reported back to the layout engine ---
    // scroll_size: how large is the chunk we actually rendered?
    AzLogicalSize scroll_size = AzLogicalSize_create(
        info.bounds.logical_size.width,
        (float)count * ROW_HEIGHT
    );
    // scroll_offset: where does this chunk sit inside the virtual space?
    AzLogicalPosition scroll_offset = AzLogicalPosition_create(
        0.0f, (float)first_row * ROW_HEIGHT
    );
    // virtual size: the full 4M-row content height
    AzLogicalSize virtual_size = AzLogicalSize_create(
        info.bounds.logical_size.width,
        (float)total * ROW_HEIGHT
    );
    AzLogicalPosition virtual_offset = AzLogicalPosition_zero();

    return AzVirtualizedViewCallbackReturn_withDom(
        dom, scroll_size, scroll_offset, virtual_size, virtual_offset
    );
}

// ---------------------------------------------------------------------------
// Root layout
// ---------------------------------------------------------------------------
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    // Title
    char title_buf[64];
    int tlen = snprintf(title_buf, sizeof(title_buf), "VirtualizedView Test - %d virtual rows", TOTAL_ROWS);
    AzString title_text = AzString_copyFromBytes((const uint8_t*)title_buf, 0, (size_t)tlen);
    AzDom title = AzDom_createDiv();
    AzDom_addChild(&title, AzDom_createText(title_text));
    AzString title_style = AzString_copyFromBytes(
        (const uint8_t*)"padding: 12px; background: #4a90d9; color: white; font-size: 18px; font-weight: bold;",
        0, 85);
    AzDom_setInlineStyle(&title, title_style);

    // VirtualizedView (the scrollable virtual list)
    AzDom vview = AzDom_createVirtualizedView(AzRefAny_clone(&data), render_rows);
    AzString vview_style = AzString_copyFromBytes(
        (const uint8_t*)"display: flex; flex-grow: 1; overflow: auto; background: #ffff00; border: 3px solid #ff00ff; margin: 8px;",
        0, 104);
    AzDom_setInlineStyle(&vview, vview_style);

    // Footer
    AzString footer_text = AzString_copyFromBytes(
        (const uint8_t*)"Scroll inside the yellow box. Only ~100 rows are rendered at a time via VirtualizedViewCallback.",
        0, 87);
    AzDom footer = AzDom_createDiv();
    AzDom_addChild(&footer, AzDom_createText(footer_text));
    AzString footer_style = AzString_copyFromBytes(
        (const uint8_t*)"padding: 8px; background: #f0f0f0; color: #666; font-size: 12px; text-align: center;",
        0, 85);
    AzDom_setInlineStyle(&footer, footer_style);

    // Body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, vview);
    AzDom_addChild(&body, footer);
    AzString body_style = AzString_copyFromBytes(
        (const uint8_t*)"display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0;",
        0, 75);
    AzDom_setInlineStyle(&body, body_style);

    return AzDom_style(&body, AzCss_empty());
}

// ---------------------------------------------------------------------------
int main(void) {
    printf("Infinity VirtualizedView Test\n");
    printf("====================\n");
    printf("Virtual rows: %d\n", TOTAL_ROWS);
    printf("Row height:   %.0f px\n", ROW_HEIGHT);
    printf("Chunk size:   %d rows\n\n", VISIBLE_ROWS);

    InfinityData model = { .total_rows = TOTAL_ROWS };
    AzRefAny data = InfinityData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AzString_copyFromBytes((const uint8_t*)"Infinity - 4M rows", 0, 18);
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
