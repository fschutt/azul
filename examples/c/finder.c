/**
 * macOS Finder Clone Demo
 *
 * Demonstrates: native menu bar, sidebar, file list, and status bar.
 *
 * Build: cc finder.c -lazul -L../../target/release/ -Wl,-rpath,../../target/release -o finder
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

typedef struct {
    size_t sidebar_selected;
    const char* current_path;
} FinderData;

void FinderData_destructor(void* ptr) { (void)ptr; }
AZ_REFLECT(FinderData, FinderData_destructor);

static AzString az(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// ---- Menu bar ----

static AzMenu build_menu_bar(void) {
    // Build submenus using arrays + copyFromPtr
    AzMenuItem file_items[] = {
        AzMenuItem_string(AzStringMenuItem_create(az("New Finder Window"))),
        AzMenuItem_string(AzStringMenuItem_create(az("New Folder"))),
        AzMenuItem_separator(),
        AzMenuItem_string(AzStringMenuItem_create(az("Get Info"))),
        AzMenuItem_separator(),
        AzMenuItem_string(AzStringMenuItem_create(az("Close Window"))),
    };
    AzStringMenuItem file_menu = AzStringMenuItem_create(az("File"));
    file_menu.children = AzMenuItemVec_copyFromPtr(file_items, 6);

    AzMenuItem edit_items[] = {
        AzMenuItem_string(AzStringMenuItem_create(az("Undo"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Redo"))),
        AzMenuItem_separator(),
        AzMenuItem_string(AzStringMenuItem_create(az("Cut"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Copy"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Paste"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Select All"))),
    };
    AzStringMenuItem edit_menu = AzStringMenuItem_create(az("Edit"));
    edit_menu.children = AzMenuItemVec_copyFromPtr(edit_items, 7);

    AzMenuItem view_items[] = {
        AzMenuItem_string(AzStringMenuItem_create(az("as Icons"))),
        AzMenuItem_string(AzStringMenuItem_create(az("as List"))),
        AzMenuItem_string(AzStringMenuItem_create(az("as Columns"))),
        AzMenuItem_string(AzStringMenuItem_create(az("as Gallery"))),
    };
    AzStringMenuItem view_menu = AzStringMenuItem_create(az("View"));
    view_menu.children = AzMenuItemVec_copyFromPtr(view_items, 4);

    AzMenuItem go_items[] = {
        AzMenuItem_string(AzStringMenuItem_create(az("Computer"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Home"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Desktop"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Downloads"))),
        AzMenuItem_string(AzStringMenuItem_create(az("Documents"))),
    };
    AzStringMenuItem go_menu = AzStringMenuItem_create(az("Go"));
    go_menu.children = AzMenuItemVec_copyFromPtr(go_items, 5);

    AzMenuItem top_items[] = {
        AzMenuItem_string(file_menu),
        AzMenuItem_string(edit_menu),
        AzMenuItem_string(view_menu),
        AzMenuItem_string(go_menu),
    };
    return AzMenu_create(AzMenuItemVec_copyFromPtr(top_items, 4));
}

// ---- File data (mock) ----

typedef struct { const char* name; const char* kind; const char* size; const char* date; } FileEntry;

static const FileEntry FILES[] = {
    { "Applications",    "Folder",     "--",       "Mar 15, 2026" },
    { "Desktop",         "Folder",     "--",       "Mar 28, 2026" },
    { "Documents",       "Folder",     "--",       "Mar 27, 2026" },
    { "Downloads",       "Folder",     "--",       "Mar 30, 2026" },
    { "report.pdf",      "PDF",        "2.4 MB",   "Mar 27, 2026" },
    { "photo.jpg",       "JPEG",       "1.1 MB",   "Mar 26, 2026" },
    { "notes.txt",       "Plain Text", "4 KB",     "Mar 25, 2026" },
    { "budget.xlsx",     "Excel",      "156 KB",   "Mar 20, 2026" },
    { "presentation.key","Keynote",    "12.3 MB",  "Mar 18, 2026" },
    { "backup.zip",      "Archive",    "450 MB",   "Mar 10, 2026" },
};
#define NUM_FILES 10

// ---- Helpers for inline CSS ----

static AzCssPropertyWithConditions prop(AzCssProperty p) {
    return AzCssPropertyWithConditions_simple(p);
}

static void set_flex_row(AzDom* d) {
    AzDom_addCssProperty(d, prop(AzCssProperty_constDisplay(AzLayoutDisplay_Flex)));
    AzDom_addCssProperty(d, prop(AzCssProperty_constFlexDirection(AzLayoutFlexDirection_Row)));
}

static void set_flex_col(AzDom* d) {
    AzDom_addCssProperty(d, prop(AzCssProperty_constDisplay(AzLayoutDisplay_Flex)));
    AzDom_addCssProperty(d, prop(AzCssProperty_constFlexDirection(AzLayoutFlexDirection_Column)));
}

static void set_flex_grow(AzDom* d, float v) {
    AzDom_addCssProperty(d, prop(AzCssProperty_constFlexGrow(AzLayoutFlexGrow_create(v))));
}

static void set_font_size(AzDom* d, float px) {
    AzDom_addCssProperty(d, prop(AzCssProperty_fontSize(AzStyleFontSize_px(px))));
}

static void set_text_color(AzDom* d, uint8_t r, uint8_t g, uint8_t b) {
    AzDom_addCssProperty(d, prop(AzCssProperty_textColor(
        (AzStyleTextColor){ .inner = AzColorU_rgb(r, g, b) })));
}

static void set_width_px(AzDom* d, float px) {
    AzDom_addCssProperty(d, prop(AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(px)))));
}

// ---- Layout ----

AzDom layout(AzRefAny data_ref, AzLayoutCallbackInfo info) {
    FinderDataRef d = FinderDataRef_create(&data_ref);
    if (!FinderData_downcastRef(&data_ref, &d)) return AzDom_createBody();
    FinderDataRef_delete(&d);

    AzDom body = AzDom_createBody();
    AzDom_setMenuBar(&body, build_menu_bar());

    // Main row: sidebar | content
    AzDom main_row = AzDom_createDiv();
    set_flex_row(&main_row);
    set_flex_grow(&main_row, 1.0);

    // -- Sidebar --
    AzDom sidebar = AzDom_createDiv();
    set_flex_col(&sidebar);
    set_width_px(&sidebar, 180.0);
    set_flex_grow(&sidebar, 0.0);

    AzDom stitle = AzDom_createP();
    AzDom_addChild(&stitle, AzDom_createText(az("Favorites")));
    set_font_size(&stitle, 11.0);
    set_text_color(&stitle, 140, 140, 140);
    AzDom_addChild(&sidebar, stitle);

    const char* sidebar_items[] = {
        "AirDrop", "Recents", "Applications", "Desktop",
        "Documents", "Downloads", "Pictures", "Music"
    };
    for (int i = 0; i < 8; i++) {
        AzDom item = AzDom_createP();
        AzDom_addChild(&item, AzDom_createText(az(sidebar_items[i])));
        set_font_size(&item, 13.0);
        AzDom_addChild(&sidebar, item);
    }
    AzDom_addChild(&main_row, sidebar);

    // -- Content --
    AzDom content = AzDom_createDiv();
    set_flex_col(&content);
    set_flex_grow(&content, 1.0);

    // Header row
    AzDom hdr = AzDom_createDiv();
    set_flex_row(&hdr);
    set_font_size(&hdr, 11.0);
    set_text_color(&hdr, 140, 140, 140);

    const char* cols[] = { "Name", "Date Modified", "Size", "Kind" };
    for (int i = 0; i < 4; i++) {
        AzDom c = AzDom_createP();
        AzDom_addChild(&c, AzDom_createText(az(cols[i])));
        set_flex_grow(&c, 1.0);
        AzDom_addChild(&hdr, c);
    }
    AzDom_addChild(&content, hdr);

    // File rows
    for (int i = 0; i < NUM_FILES; i++) {
        AzDom row = AzDom_createDiv();
        set_flex_row(&row);
        set_font_size(&row, 13.0);

        const char* cells[] = { FILES[i].name, FILES[i].date, FILES[i].size, FILES[i].kind };
        for (int j = 0; j < 4; j++) {
            AzDom cell = AzDom_createP();
            AzDom_addChild(&cell, AzDom_createText(az(cells[j])));
            set_flex_grow(&cell, 1.0);
            AzDom_addChild(&row, cell);
        }
        AzDom_addChild(&content, row);
    }
    AzDom_addChild(&main_row, content);
    AzDom_addChild(&body, main_row);

    // Status bar
    char status[64];
    int slen = snprintf(status, sizeof(status), "%d items", NUM_FILES);
    AzDom sbar = AzDom_createP();
    AzDom_addChild(&sbar, AzDom_createText(AzString_copyFromBytes((const uint8_t*)status, 0, slen)));
    set_font_size(&sbar, 11.0);
    set_text_color(&sbar, 140, 140, 140);
    AzDom_addChild(&body, sbar);

    return body;
}

int main() {
    FinderData model = { .sidebar_selected = 0, .current_path = "/Users/demo" };
    AzRefAny data = FinderData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = az("Finder");
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 600.0;

    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
