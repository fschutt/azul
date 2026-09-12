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
    uint32_t expanded_nodes;
    size_t selected_node;
    size_t sort_column;
} WidgetShowcase;

void WidgetShowcase_destructor(void* m) { }
AZ_REFLECT(WidgetShowcase, WidgetShowcase_destructor);

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);
AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info, AzCheckBoxState state);
AzUpdate on_list_row_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t row_index);
AzUpdate on_tree_node_click(AzRefAny data, AzCallbackInfo info, size_t node_index);
AzUpdate on_tab_click(AzRefAny data, AzCallbackInfo info, size_t tab_index);
AzUpdate on_list_column_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t column_index);

#define FILE_COUNT 8

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

/* The widget THEME, one constant for the whole showcase: the checkbox
 * switches it. Unchecked = the default theme (`None` lets each widget
 * pick its own default), checked = Flora. Every themed widget below reads
 * this one value, so a toggle re-skins the whole window through
 * `AzUpdate_RefreshDom`. */
static AzOptionUiTheme theme_for(bool flora) {
    AzOptionUiTheme theme;
    if (flora) {
        theme.Some.tag = AzOptionUiTheme_Tag_Some;
        theme.Some.payload = AzUiTheme_Flora;
    } else {
        theme.None.tag = AzOptionUiTheme_Tag_None;
    }
    return theme;
}

static AzOptionUsize some_usize(size_t value) {
    AzOptionUsize opt;
    opt.Some.tag = AzOptionUsize_Tag_Some;
    opt.Some.payload = value;
    return opt;
}

static void sorted_order(const char* rows[FILE_COUNT][3], size_t column, size_t* out) {
    for (size_t i = 0; i < FILE_COUNT; i++) {
        out[i] = i;
    }
    for (size_t i = 1; i < FILE_COUNT; i++) {
        size_t key = out[i];
        size_t j = i;
        while (j > 0 && strcmp(rows[out[j - 1]][column], rows[key][column]) > 0) {
            out[j] = out[j - 1];
            j--;
        }
        out[j] = key;
    }
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
    /* The checkbox is the theme switch: checked = Flora. */
    AzOptionUiTheme theme = theme_for(checked);

    AzButton btn = AzButton_create(str("Click me!"));
    btn.theme = theme;
    AzButton_setOnClick(&btn, AzRefAny_clone(&data), on_button_click);
    AzDom button = AzButton_dom(btn);
    AzDom_setCss(&button, str("margin-bottom: 10px;"));

    AzCheckBox cb = AzCheckBox_create(checked);
    cb.theme = theme;
    AzCheckBox_setOnToggle(&cb, AzRefAny_clone(&data), on_checkbox_toggle);
    AzDom checkbox = AzCheckBox_dom(cb);
    AzDom_setCss(&checkbox, str("margin-bottom: 10px;"));

    AzProgressBar pb = AzProgressBar_create(progress_value);
    pb.theme = theme;
    AzDom progress = AzProgressBar_dom(pb);
    AzDom_setCss(&progress, str("margin-bottom: 10px;"));

    AzTextInput ti = AzTextInput_create();
    ti = AzTextInput_withPlaceholder(ti, str("Enter text here..."));
    ti.theme = theme;
    AzDom text_input = AzTextInput_dom(ti);
    AzDom_setCss(&text_input, str("margin-bottom: 10px;"));

    AzColorU color = { .r = 100, .g = 150, .b = 200, .a = 255 };
    AzDom color_input = AzColorInput_dom(AzColorInput_create(color));
    AzDom_setCss(&color_input, str("margin-bottom: 10px;"));

    AzDom number_input = AzNumberInput_dom(AzNumberInput_create(42.0));
    AzDom_setCss(&number_input, str("margin-bottom: 10px;"));

    static const char* row_data[FILE_COUNT][3] = {
        { "report.pdf",     "120 KB", "PDF"     },
        { "photo.png",      "2.4 MB", "Image"   },
        { "notes.txt",      "4 KB",   "Text"    },
        { "archive.zip",    "88 MB",  "Archive" },
        { "slides.key",     "12 MB",  "Slides"  },
        { "budget.numbers", "340 KB", "Sheet"   },
        { "logo.svg",       "18 KB",  "Vector"  },
        { "readme.md",      "2 KB",   "Text"    },
    };
    size_t order[FILE_COUNT];
    sorted_order(row_data, d.ptr->sort_column, order);

    AzString col_names[3] = { str("Name"), str("Size"), str("Type") };
    AzListView lv = AzListView_create(AzStringVec_copyFromPtr(col_names, 3));
    AzListViewRow rows[FILE_COUNT];
    for (size_t r = 0; r < FILE_COUNT; r++) {
        AzDom cells[3];
        for (size_t c = 0; c < 3; c++) {
            cells[c] = AzDom_createSpanWithText(str(row_data[order[r]][c]));
        }
        rows[r].cells = AzDomVec_copyFromPtr(cells, 3);
        rows[r].height.None.tag = AzOptionPixelValueNoPercent_Tag_None;
    }
    AzListView_setRows(&lv, AzListViewRowVec_copyFromPtr(rows, FILE_COUNT));
    AzListView_setSortedBy(&lv, some_usize(d.ptr->sort_column));
    AzListView_setOnRowClick(&lv, AzRefAny_clone(&data), on_list_row_click);
    AzListView_setOnColumnClick(&lv, AzRefAny_clone(&data), on_list_column_click);
    AzDom list_view = AzListView_dom(lv);
    AzDom_setCss(&list_view, str("flex-grow: 1; overflow-y: auto;"));

    uint32_t expanded = d.ptr->expanded_nodes;
    size_t selected_node = d.ptr->selected_node;

    static const char* tree_labels[7] = {
        "Home", "Documents", "report.pdf", "photo.png", "notes.txt", "Downloads", "azul-0.2.0.tar.gz",
    };
    AzTreeViewNode nodes[7];
    for (size_t i = 0; i < 7; i++) {
        nodes[i] = AzTreeViewNode_new(str(tree_labels[i]));
        nodes[i] = AzTreeViewNode_withExpanded(nodes[i], (expanded & (1u << i)) != 0);
        nodes[i] = AzTreeViewNode_withSelected(nodes[i], i == selected_node);
    }
    AzTreeViewNode_addChild(&nodes[1], nodes[2]);
    AzTreeViewNode_addChild(&nodes[1], nodes[3]);
    AzTreeViewNode_addChild(&nodes[1], nodes[4]);
    AzTreeViewNode_addChild(&nodes[5], nodes[6]);
    AzTreeViewNode_addChild(&nodes[0], nodes[1]);
    AzTreeViewNode_addChild(&nodes[0], nodes[5]);
    AzTreeViewNode root = nodes[0];

    AzTreeView tv = AzTreeView_new(root);
    AzTreeView_setOnNodeClick(&tv, AzRefAny_clone(&data), on_tree_node_click);
    AzDom tree_view = AzTreeView_dom(tv);
    AzDom_setCss(&tree_view, str("width: 200px; margin-right: 10px;"));

    AzDom browser = AzDom_createDiv();
    AzDom_setCss(&browser, str("display: flex; flex-direction: row; height: 150px; margin-bottom: 10px;"));
    AzDom_addChild(&browser, tree_view);
    AzDom_addChild(&browser, list_view);

    AzDom content = AzDom_createDiv();
    /* No hardcoded background: the window's own background follows the system
     * theme, and painting white over it left a light panel full of dark
     * widgets on a dark desktop. */
    AzDom_setCss(&content, str("flex-grow: 1; padding: 20px; overflow: auto;"));
    AzDom_addChild(&content, button);
    AzDom_addChild(&content, checkbox);
    AzDom_addChild(&content, progress);
    AzDom_addChild(&content, text_input);
    AzDom_addChild(&content, color_input);
    AzDom_addChild(&content, number_input);
    AzDom_addChild(&content, browser);

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

AzUpdate on_tree_node_click(AzRefAny data, AzCallbackInfo info, size_t node_index) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->selected_node = node_index;
    d.ptr->expanded_nodes ^= (1u << node_index);
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_list_column_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t column_index) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->sort_column = column_index;
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
        .selected_row = 0,
        .expanded_nodes = (1u << 0) | (1u << 1),
        .selected_node = 2,
        .sort_column = 0
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
