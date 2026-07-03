// cc -o widgets widgets.c -lazul

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
AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info);
AzUpdate on_list_row_click(AzRefAny data, AzCallbackInfo info, AzListViewState state, size_t row_index);

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    WidgetShowcaseRef d = WidgetShowcaseRef_create(&data);
    if (!WidgetShowcase_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    // Create button
    AzDom button = AzDom_createDiv();
    AzString btn_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&button, btn_style);
    AzString btn_text = AzString_copyFromBytes((const uint8_t*)"Click me!", 0, 9);
    AzDom_addChild(&button, AzDom_createText(btn_text));
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    AzDom_addCallback(&button, event, AzRefAny_clone(&data), on_button_click);

    // Create checkbox
    AzDom checkbox = AzCheckBox_dom(AzCheckBox_create(d.ptr->checkbox_checked));
    AzString chk_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&checkbox, chk_style);

    // Create progress bar
    AzDom progress = AzProgressBar_dom(AzProgressBar_create(d.ptr->progress_value));
    AzString prog_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&progress, prog_style);

    // Create text input
    AzString placeholder = AzString_copyFromBytes((const uint8_t*)"Enter text here...", 0, 18);
    AzTextInput ti = AzTextInput_create();
    ti = AzTextInput_withPlaceholder(ti, placeholder);
    AzDom text_input = AzTextInput_dom(ti);
    AzString txt_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&text_input, txt_style);

    // Create color input
    AzColorU color = { .r = 100, .g = 150, .b = 200, .a = 255 };
    AzDom color_input = AzColorInput_dom(AzColorInput_create(color));
    AzString col_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&color_input, col_style);

    // Create number input
    AzDom number_input = AzNumberInput_dom(AzNumberInput_create(42.0));
    AzString num_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setCss(&number_input, num_style);

    // Create list view with clickable rows (on_row_click gets the row index)
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
            cells[c] = AzDom_createText(str(row_data[r][c]));
        }
        rows[r].cells = AzDomVec_copyFromPtr(cells, 3);
        rows[r].height.None.tag = AzOptionPixelValueNoPercent_Tag_None;
    }
    AzListView_setRows(&lv, AzListViewRowVec_copyFromPtr(rows, 3));
    AzListView_setOnRowClick(&lv, AzRefAny_clone(&data), on_list_row_click);
    AzDom list_view = AzListView_dom(lv);
    AzDom_setCss(&list_view, str("height: 150px; margin-bottom: 10px;"));

    // Compose body
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"padding: 20px;", 0, 14);
    AzDom_setCss(&body, body_style);
    AzString showcase_title = AzString_copyFromBytes((const uint8_t*)"Widget Showcase", 0, 15);

    AzDom_addChild(&body, AzDom_createText(showcase_title));
    AzDom_addChild(&body, button);
    AzDom_addChild(&body, checkbox);
    AzDom_addChild(&body, progress);
    AzDom_addChild(&body, text_input);
    AzDom_addChild(&body, color_input);
    AzDom_addChild(&body, number_input);
    AzDom_addChild(&body, list_view);

    WidgetShowcaseRef_delete(&d);
    return body;
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
    printf("row %zu clicked\n", row_index);

    // Headless measure: lay out a DOM off-screen to get its natural size
    // (the building block for virtual-list item sizing)
    AzDom probe = AzDom_createText(str("How tall is this text at 200px width?"));
    AzLogicalSize avail = { .width = 200.0f, .height = 1000000.0f };
    AzLogicalSize measured = AzCallbackInfo_measureDom(&info, probe, avail);
    printf("probe DOM measures %.1f x %.1f px\n", measured.width, measured.height);

    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(&data);
    if (!WidgetShowcase_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->checkbox_checked = !d.ptr->checkbox_checked;
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
    AzString window_title = AzString_copyFromBytes("Widget Showcase", 0, 15);
    window.window_state.title = window_title;
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
