// Widgets Showcase - C
// cc -o widgets widgets.c -lazul

#include "azul.h"
#include <stdio.h>

typedef struct {
    bool enable_padding;
    size_t active_tab;
    float progress_value;
    bool checkbox_checked;
    char text_input[256];
} WidgetShowcase;

void WidgetShowcase_destructor(void* m) { }
AZ_REFLECT(WidgetShowcase, WidgetShowcase_destructor);

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);
AzUpdate on_checkbox_toggle(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    WidgetShowcaseRef d = WidgetShowcaseRef_create(&data);
    if (!WidgetShowcase_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }

    // Create button
    AzDom button = AzDom_createDiv();
    AzString btn_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&button, btn_style);
    AzString btn_text = AzString_copyFromBytes((const uint8_t*)"Click me!", 0, 9);
    AzDom_addChild(&button, AzDom_createText(btn_text));
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    AzDom_addCallback(&button, event, AzRefAny_clone(&data), on_button_click);

    // Create checkbox
    AzDom checkbox = AzCheckBox_dom(AzCheckBox_create(d.ptr->checkbox_checked));
    AzString chk_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&checkbox, chk_style);

    // Create progress bar
    AzDom progress = AzProgressBar_dom(AzProgressBar_create(d.ptr->progress_value));
    AzString prog_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&progress, prog_style);

    // Create text input
    AzString placeholder = AzString_copyFromBytes((const uint8_t*)"Enter text here...", 0, 18);
    AzTextInput ti = AzTextInput_create();
    ti = AzTextInput_withPlaceholder(ti, placeholder);
    AzDom text_input = AzTextInput_dom(ti);
    AzString txt_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&text_input, txt_style);

    // Create color input
    AzColorU color = { .r = 100, .g = 150, .b = 200, .a = 255 };
    AzDom color_input = AzColorInput_dom(AzColorInput_create(color));
    AzString col_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&color_input, col_style);

    // Create number input
    AzDom number_input = AzNumberInput_dom(AzNumberInput_create(42.0));
    AzString num_style = AzString_copyFromBytes((const uint8_t*)"margin-bottom: 10px;", 0, 20);
    AzDom_setInlineStyle(&number_input, num_style);

    // Compose body
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"padding: 20px;", 0, 14);
    AzDom_setInlineStyle(&body, body_style);
    AzString showcase_title = AzString_copyFromBytes((const uint8_t*)"Widget Showcase", 0, 15);

    AzDom_addChild(&body, AzDom_createText(showcase_title));
    AzDom_addChild(&body, button);
    AzDom_addChild(&body, checkbox);
    AzDom_addChild(&body, progress);
    AzDom_addChild(&body, text_input);
    AzDom_addChild(&body, color_input);
    AzDom_addChild(&body, number_input);

    WidgetShowcaseRef_delete(&d);
    return AzDom_style(&body, AzCss_empty());
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
        .text_input = ""
    };
    AzRefAny data = WidgetShowcase_upcast(model);
    
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString window_title = AzString_copyFromBytes("Widget Showcase", 0, 15);
    window.window_state.title = window_title;
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 500.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
