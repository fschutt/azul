#include "azul.h"
#include <stdio.h>
#include <string.h>

#define TOTAL_ROWS      1000000
#define ROW_HEIGHT      22.0f
#define VISIBLE_ROWS    48
#define COL_COUNT       12
#define COL_WIDTH       92.0f
#define ROW_HEAD_WIDTH  52.0f

typedef struct {
    int total_rows;
} SheetData;

void SheetData_destructor(void* d) { }
AZ_REFLECT(SheetData, SheetData_destructor);

static const char* PRODUCTS[8] = {
    "Widget", "Gasket", "Flange", "Bearing",
    "Bracket", "Spindle", "Coupler", "Sleeve"
};

static const char* REGIONS[5] = { "North", "South", "East", "West", "Central" };

static const char* COL_LABELS[COL_COUNT] = {
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"
};

static const char* COL_TITLES[COL_COUNT] = {
    "Order", "Product", "Region", "Qty", "Unit", "Net",
    "Tax", "Total", "Q1", "Q2", "Q3", "Q4"
};

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzDom cell(const char* text, const char* css) {
    AzDom d = AzDom_createDiv();
    AzDom_addChild(&d, AzDom_createPWithText(str(text)));
    AzDom_setCss(&d, str(css));
    return d;
}

static uint32_t hash2(int row, int col) {
    uint32_t h = (uint32_t)row * 2654435761u ^ ((uint32_t)col * 40503u);
    h ^= h >> 13;
    h *= 1274126177u;
    h ^= h >> 16;
    return h;
}

static void cell_text(char* buf, size_t cap, int row, int col) {
    uint32_t h = hash2(row, col);
    switch (col) {
        case 0: snprintf(buf, cap, "SO-%06d", 100000 + row); break;
        case 1: snprintf(buf, cap, "%s", PRODUCTS[h % 8]); break;
        case 2: snprintf(buf, cap, "%s", REGIONS[h % 5]); break;
        case 3: snprintf(buf, cap, "%u", (h % 90) + 10); break;
        default: snprintf(buf, cap, "%u.%02u", h % 900 + 10, h % 100); break;
    }
}

AzVirtualViewReturn render_rows(AzRefAny data, AzVirtualViewCallbackInfo info) {

    SheetDataRef d = SheetDataRef_create(&data);
    if (!SheetData_downcastRef(&data, &d)) {
        return AzVirtualViewReturn_withDom(
            AzDom_createBody(),
            AzLogicalRect_create(AzLogicalPosition_zero(), AzLogicalSize_zero()),
            AzLogicalRect_create(AzLogicalPosition_zero(), AzLogicalSize_zero())
        );
    }

    int total = d.ptr->total_rows;
    SheetDataRef_delete(&d);

    float scroll_y = info.scroll_offset.y;
    if (scroll_y < 0.0f) scroll_y = 0.0f;

    int first_row = (int)(scroll_y / ROW_HEIGHT);
    if (first_row < 0) first_row = 0;
    if (first_row >= total) first_row = total - 1;

    int count = VISIBLE_ROWS;
    if (first_row + count > total) count = total - first_row;

    AzDom container = AzDom_createDiv();

    char css[320];
    char text[64];

    for (int i = 0; i < count; i++) {
        int row_idx = first_row + i;
        const char* band = (row_idx % 2 == 0) ? "#ffffff" : "#f6f8fb";

        AzDom row = AzDom_createDiv();
        snprintf(css, sizeof(css),
            "display: flex; flex-direction: row; height: %.0fpx; background: %s;",
            ROW_HEIGHT, band);
        AzDom_setCss(&row, str(css));

        snprintf(text, sizeof(text), "%d", row_idx + 1);
        snprintf(css, sizeof(css),
            "width: %.0fpx; min-width: %.0fpx; height: %.0fpx; line-height: %.0fpx; "
            "text-align: center; font-size: 11px; color: #444444; background: #eceff4; "
            "border-right: 1px solid #b6bcc6; border-bottom: 1px solid #d7dbe2;",
            ROW_HEAD_WIDTH, ROW_HEAD_WIDTH, ROW_HEIGHT, ROW_HEIGHT);
        AzDom_addChild(&row, cell(text, css));

        for (int c = 0; c < COL_COUNT; c++) {
            cell_text(text, sizeof(text), row_idx, c);
            const char* align = (c >= 3) ? "right" : "left";
            snprintf(css, sizeof(css),
                "width: %.0fpx; min-width: %.0fpx; height: %.0fpx; line-height: %.0fpx; "
                "padding-left: 6px; padding-right: 6px; font-size: 12px; color: #1f2933; "
                "text-align: %s; overflow: hidden; "
                "border-right: 1px solid #d7dbe2; border-bottom: 1px solid #d7dbe2;",
                COL_WIDTH, COL_WIDTH, ROW_HEIGHT, ROW_HEIGHT, align);
            AzDom_addChild(&row, cell(text, css));
        }

        AzDom_addChild(&container, row);
    }

    float sheet_width = ROW_HEAD_WIDTH + COL_COUNT * COL_WIDTH;

    AzLogicalSize scroll_size = AzLogicalSize_create(sheet_width, (float)count * ROW_HEIGHT);
    AzLogicalPosition scroll_offset = AzLogicalPosition_create(0.0f, (float)first_row * ROW_HEIGHT);
    AzLogicalSize virtual_size = AzLogicalSize_create(sheet_width, (float)total * ROW_HEIGHT);
    AzLogicalPosition virtual_offset = AzLogicalPosition_zero();

    return AzVirtualViewReturn_withDom(
        container,
        AzLogicalRect_create(scroll_offset, scroll_size),
        AzLogicalRect_create(virtual_offset, virtual_size)
    );
}

static AzDom column_header(void) {
    AzDom header = AzDom_createDiv();
    AzDom_setCss(&header, str(
        "display: flex; flex-direction: row; background: #dfe3ea; "
        "border-bottom: 1px solid #9aa2ae;"));

    char css[320];
    char text[64];

    snprintf(css, sizeof(css),
        "width: %.0fpx; min-width: %.0fpx; height: 24px; line-height: 24px; "
        "border-right: 1px solid #9aa2ae; background: #d3d8e0;",
        ROW_HEAD_WIDTH, ROW_HEAD_WIDTH);
    AzDom_addChild(&header, cell("", css));

    for (int c = 0; c < COL_COUNT; c++) {
        snprintf(text, sizeof(text), "%s   %s", COL_LABELS[c], COL_TITLES[c]);
        snprintf(css, sizeof(css),
            "width: %.0fpx; min-width: %.0fpx; height: 24px; line-height: 24px; "
            "padding-left: 6px; font-size: 11px; font-weight: bold; color: #33404f; "
            "overflow: hidden; border-right: 1px solid #9aa2ae;",
            COL_WIDTH, COL_WIDTH);
        AzDom_addChild(&header, cell(text, css));
    }

    return header;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    char buf[96];
    snprintf(buf, sizeof(buf), "Sheet1  -  %d rows x %d columns", TOTAL_ROWS, COL_COUNT);
    AzDom title = AzDom_createDiv();
    AzDom_addChild(&title, AzDom_createPWithText(str(buf)));
    AzDom_setCss(&title, str(
        "padding: 8px 12px; background: #217346; color: white; "
        "font-size: 13px; font-weight: bold;"));

    AzDom vview = AzDom_createVirtualView(AzRefAny_clone(&data), render_rows);
    AzDom_setCss(&vview, str(
        "display: flex; flex-grow: 1; overflow-y: auto; overflow-x: hidden; "
        "background: #ffffff;"));

    AzDom status = AzDom_createDiv();
    AzDom_addChild(&status, AzDom_createPWithText(
        str("Ready   -   only the visible band of cells exists in the DOM")));
    AzDom_setCss(&status, str(
        "padding: 4px 12px; background: #f1f3f6; border-top: 1px solid #c9ced6; "
        "color: #55606e; font-size: 11px;"));

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, column_header());
    AzDom_addChild(&body, vview);
    AzDom_addChild(&body, status);
    AzDom_setCss(&body, str(
        "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
        "font-family: sans-serif; background: #ffffff;"));

    return body;
}

int main(void) {
    SheetData model = { .total_rows = TOTAL_ROWS };
    AzRefAny data = SheetData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = str("Infinity - 1M row spreadsheet");
    window.window_state.size.dimensions.width = ROW_HEAD_WIDTH + COL_COUNT * COL_WIDTH + 18.0f;
    window.window_state.size.dimensions.height = 620.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
