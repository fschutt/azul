#include "azul.h"
#include <stdio.h>
#include <string.h>

typedef struct {
    bool enable_padding;
    size_t active_tab;
    float progress_value;
    bool checkbox_checked;
    char text_input[256];
    size_t selected_row;
} WidgetShowcase;

void WidgetShowcase_destructor(void* m) { }
AZ_REFLECT(WidgetShowcase, WidgetShowcase_destructor);

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);
AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info, AzCheckBoxState state);
AzUpdate on_list_row_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t row_index);
AzUpdate on_tab_click(AzRefAny data, AzCallbackInfo info, size_t tab_index);

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzRibbonItem small_button(const char* icon, const char* label) {
    return AzRibbonItem_smallButton(AzRibbonButton_new(str(icon), str(label)));
}

static AzRibbonItem menu_button(const char* icon, const char* label) {
    AzRibbonButton b = AzRibbonButton_new(str(icon), str(label));
    return AzRibbonItem_smallButton(AzRibbonButton_withArrow(b, AzRibbonArrow_Menu));
}

static AzRibbonItem large_button(const char* icon, const char* label, AzRibbonArrow arrow) {
    AzRibbonButton b = AzRibbonButton_new(str(icon), str(label));
    return AzRibbonItem_largeButton(AzRibbonButton_withArrow(b, arrow));
}

static AzRibbonItem column_of(const AzRibbonItem* items, size_t count) {
    AzRibbonColumn col = AzRibbonColumn_new();
    for (size_t i = 0; i < count; i++) {
        AzRibbonColumn_addItem(&col, items[i]);
    }
    return AzRibbonItem_column(col);
}

static AzRibbonItem row_of(const AzRibbonItem* items, size_t count) {
    AzRibbonRow row = AzRibbonRow_new();
    for (size_t i = 0; i < count; i++) {
        AzRibbonRow_addItem(&row, items[i]);
    }
    return AzRibbonItem_row(row);
}

static AzRibbonGroup group_of(const char* label, const AzRibbonItem* items, size_t count) {
    AzRibbonGroup g = AzRibbonGroup_new(str(label));
    for (size_t i = 0; i < count; i++) {
        AzRibbonGroup_addItem(&g, items[i]);
    }
    return g;
}

static AzRibbonTab home_tab(void) {
    AzRibbonItem clipboard_col[3] = {
        small_button("content_cut", "Cut"),
        small_button("content_copy", "Copy"),
        small_button("format_paint", "Format Painter"),
    };
    AzRibbonItem clipboard_items[2] = {
        large_button("content_paste", "Paste", AzRibbonArrow_Split),
        column_of(clipboard_col, 3),
    };

    AzRibbonItem font_row1[4] = {
        small_button("text_increase", ""),
        small_button("text_decrease", ""),
        menu_button("text_fields", ""),
        small_button("format_clear", ""),
    };
    AzRibbonItem font_row2[6] = {
        small_button("format_bold", ""),
        small_button("format_italic", ""),
        small_button("format_underlined", ""),
        small_button("strikethrough_s", ""),
        AzRibbonItem_separator(),
        menu_button("format_color_text", ""),
    };
    AzRibbonItem font_col[2] = { row_of(font_row1, 4), row_of(font_row2, 6) };
    AzRibbonItem font_items[1] = { column_of(font_col, 2) };

    AzRibbonItem para_row1[5] = {
        menu_button("format_list_bulleted", ""),
        menu_button("format_list_numbered", ""),
        AzRibbonItem_separator(),
        small_button("format_indent_decrease", ""),
        small_button("format_indent_increase", ""),
    };
    AzRibbonItem para_row2[5] = {
        small_button("format_align_left", ""),
        small_button("format_align_center", ""),
        small_button("format_align_right", ""),
        AzRibbonItem_separator(),
        menu_button("format_line_spacing", ""),
    };
    AzRibbonItem para_col[2] = { row_of(para_row1, 5), row_of(para_row2, 5) };
    AzRibbonItem para_items[1] = { column_of(para_col, 2) };

    AzRibbonItem editing_col[3] = {
        menu_button("search", "Find"),
        small_button("find_replace", "Replace"),
        menu_button("highlight_alt", "Select"),
    };
    AzRibbonItem editing_items[1] = { column_of(editing_col, 3) };

    AzRibbonGroup groups[4] = {
        group_of("Clipboard", clipboard_items, 2),
        group_of("Font", font_items, 1),
        group_of("Paragraph", para_items, 1),
        group_of("Editing", editing_items, 1),
    };

    AzRibbonTab tab = AzRibbonTab_new(str("HOME"));
    for (size_t i = 0; i < 4; i++) {
        AzRibbonTab_addGroup(&tab, groups[i]);
    }
    return tab;
}

static AzRibbonTab insert_tab(void) {
    AzRibbonItem table_items[1] = { large_button("grid_on", "Table", AzRibbonArrow_Menu) };
    AzRibbonItem media_items[3] = {
        large_button("image", "Pictures", AzRibbonArrow_None),
        large_button("insert_chart", "Chart", AzRibbonArrow_None),
        large_button("category", "Shapes", AzRibbonArrow_Menu),
    };
    AzRibbonGroup groups[2] = {
        group_of("Tables", table_items, 1),
        group_of("Illustrations", media_items, 3),
    };
    AzRibbonTab tab = AzRibbonTab_new(str("INSERT"));
    for (size_t i = 0; i < 2; i++) {
        AzRibbonTab_addGroup(&tab, groups[i]);
    }
    return tab;
}

static AzRibbonTab view_tab(void) {
    AzRibbonItem views[3] = {
        large_button("article", "Read Mode", AzRibbonArrow_None),
        large_button("description", "Print Layout", AzRibbonArrow_None),
        large_button("public", "Web Layout", AzRibbonArrow_None),
    };
    AzRibbonItem zoom[2] = {
        large_button("zoom_in", "Zoom", AzRibbonArrow_None),
        large_button("fit_screen", "One Page", AzRibbonArrow_None),
    };
    AzRibbonGroup groups[2] = {
        group_of("Views", views, 3),
        group_of("Zoom", zoom, 2),
    };
    AzRibbonTab tab = AzRibbonTab_new(str("VIEW"));
    for (size_t i = 0; i < 2; i++) {
        AzRibbonTab_addGroup(&tab, groups[i]);
    }
    return tab;
}

static AzDom ribbon_dom(AzRefAny data, size_t active_tab) {
    AzRibbonTab tabs[3] = { home_tab(), insert_tab(), view_tab() };
    AzRibbon ribbon = AzRibbon_new(AzRibbonTabVec_copyFromPtr(tabs, 3));
    AzRibbon_setAppButton(&ribbon, AzRibbonAppButton_new(str("FILE")));
    AzRibbon_setActiveTab(&ribbon, active_tab);
    AzRibbon_setOnTabClick(&ribbon, data, on_tab_click);
    return AzRibbon_dom(ribbon);
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    WidgetShowcaseRef d = WidgetShowcaseRef_create(&data);
    if (!WidgetShowcase_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    size_t active_tab = d.ptr->active_tab;
    bool checked = d.ptr->checkbox_checked;
    float progress_value = d.ptr->progress_value;

    AzDom button = AzDom_createDiv();
    AzDom_setCss(&button, str("margin-bottom: 10px;"));
    AzDom_addChild(&button, AzDom_createTextDoNotUseWithoutBlockLevelWrapper(str("Click me!")));
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    AzDom_addCallback(&button, event, AzRefAny_clone(&data), on_button_click);

    AzCheckBox cb = AzCheckBox_create(checked);
    AzCheckBox_setOnToggle(&cb, AzRefAny_clone(&data), on_checkbox_toggle);
    AzDom checkbox = AzCheckBox_dom(cb);
    AzDom_setCss(&checkbox, str("margin-bottom: 10px;"));

    AzDom progress = AzProgressBar_dom(AzProgressBar_create(progress_value));
    AzDom_setCss(&progress, str("margin-bottom: 10px;"));

    AzTextInput ti = AzTextInput_create();
    ti = AzTextInput_withPlaceholder(ti, str("Enter text here..."));
    AzDom text_input = AzTextInput_dom(ti);
    AzDom_setCss(&text_input, str("margin-bottom: 10px;"));

    AzColorU color = { .r = 100, .g = 150, .b = 200, .a = 255 };
    AzDom color_input = AzColorInput_dom(AzColorInput_create(color));
    AzDom_setCss(&color_input, str("margin-bottom: 10px;"));

    AzDom number_input = AzNumberInput_dom(AzNumberInput_create(42.0));
    AzDom_setCss(&number_input, str("margin-bottom: 10px;"));

    static const char* row_data[3][3] = {
        { "report.pdf",  "120 KB", "PDF"   },
        { "photo.png",   "2.4 MB", "Image" },
        { "notes.txt",   "4 KB",   "Text"  },
    };
    AzString col_names[3] = { str("Name"), str("Size"), str("Type") };
    AzListView lv = AzListView_create(AzStringVec_copyFromPtr(col_names, 3));
    AzListViewRow rows[3];
    for (size_t r = 0; r < 3; r++) {
        AzDom cells[3];
        for (size_t c = 0; c < 3; c++) {
            cells[c] = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(str(row_data[r][c]));
        }
        rows[r].cells = AzDomVec_copyFromPtr(cells, 3);
        rows[r].height.None.tag = AzOptionPixelValueNoPercent_Tag_None;
    }
    AzListView_setRows(&lv, AzListViewRowVec_copyFromPtr(rows, 3));
    AzListView_setOnRowClick(&lv, AzRefAny_clone(&data), on_list_row_click);
    AzDom list_view = AzListView_dom(lv);
    AzDom_setCss(&list_view, str("height: 150px; margin-bottom: 10px;"));

    AzDom content = AzDom_createDiv();
    AzDom_setCss(&content, str("flex-grow: 1; padding: 20px; overflow: auto; background: white;"));
    AzDom_addChild(&content, button);
    AzDom_addChild(&content, checkbox);
    AzDom_addChild(&content, progress);
    AzDom_addChild(&content, text_input);
    AzDom_addChild(&content, color_input);
    AzDom_addChild(&content, number_input);
    AzDom_addChild(&content, list_view);

    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, str("display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0;"));
    AzDom_addChild(&body, ribbon_dom(AzRefAny_clone(&data), active_tab));
    AzDom_addChild(&body, content);

    WidgetShowcaseRef_delete(&d);
    return body;
}

AzUpdate on_tab_click(AzRefAny data, AzCallbackInfo info, size_t tab_index) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->active_tab = tab_index;
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->progress_value += 10.0;
    if (d.ptr->progress_value > 100.0) {
        d.ptr->progress_value = 0.0;
    }
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_list_row_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t row_index) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->selected_row = row_index;
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info, AzCheckBoxState state) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->checkbox_checked = state.checked;
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    WidgetShowcase model = {
        .enable_padding = true,
        .active_tab = 0,
        .progress_value = 25.0,
        .checkbox_checked = false,
        .text_input = "",
        .selected_row = 0
    };
    AzRefAny data = WidgetShowcase_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = str("Azul Widgets");
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 620.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
