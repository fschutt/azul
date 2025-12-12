// Widgets Showcase - C
// cc -o widgets widgets.c -lazul

#include <azul.h>
#include <stdio.h>

typedef struct {
    bool enable_padding;
    size_t active_tab;
    float progress_value;
    bool checkbox_checked;
    char text_input[256];
} WidgetShowcase;

void WidgetShowcase_destructor(WidgetShowcase* m) { }
AZ_REFLECT(WidgetShowcase, WidgetShowcase_destructor);

AzUpdate on_button_click(AzRefAny* data, AzCallbackInfo* info);
AzUpdate on_checkbox_toggle(AzRefAny* data, AzCallbackInfo* info);

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    WidgetShowcaseRef d = WidgetShowcaseRef_create(data);
    if (!WidgetShowcase_downcastRef(data, &d)) {
        return AzStyledDom_default();
    }

    // Create button
    AzDom button = AzDom_div();
    AzDom_setInlineStyle(&button, AzString_fromConstStr("margin-bottom: 10px;"));
    AzDom_addChild(&button, AzDom_text(AzString_fromConstStr("Click me!")));
    AzDom_setCallback(&button, AzOn_MouseUp, AzRefAny_clone(data), on_button_click);

    // Create checkbox
    AzDom checkbox = AzCheckBox_dom(AzCheckBox_new(d.ptr->checkbox_checked));
    AzDom_setInlineStyle(&checkbox, AzString_fromConstStr("margin-bottom: 10px;"));

    // Create progress bar
    AzDom progress = AzProgressBar_dom(AzProgressBar_new(d.ptr->progress_value));
    AzDom_setInlineStyle(&progress, AzString_fromConstStr("margin-bottom: 10px;"));

    // Create text input
    AzDom text_input = AzTextInput_dom(AzTextInput_new(
        AzString_fromConstStr("Enter text here...")
    ));
    AzDom_setInlineStyle(&text_input, AzString_fromConstStr("margin-bottom: 10px;"));

    // Create color input
    AzColorU color = { .r = 100, .g = 150, .b = 200, .a = 255 };
    AzDom color_input = AzColorInput_dom(AzColorInput_new(color));
    AzDom_setInlineStyle(&color_input, AzString_fromConstStr("margin-bottom: 10px;"));

    // Create number input
    AzDom number_input = AzNumberInput_dom(AzNumberInput_new(42.0));
    AzDom_setInlineStyle(&number_input, AzString_fromConstStr("margin-bottom: 10px;"));

    // Compose body
    AzDom body = AzDom_body();
    AzDom_setInlineStyle(&body, AzString_fromConstStr("padding: 20px;"));
    AzDom_addChild(&body, AzDom_text(AzString_fromConstStr("Widget Showcase")));
    AzDom_addChild(&body, button);
    AzDom_addChild(&body, checkbox);
    AzDom_addChild(&body, progress);
    AzDom_addChild(&body, text_input);
    AzDom_addChild(&body, color_input);
    AzDom_addChild(&body, number_input);

    WidgetShowcaseRef_delete(&d);
    return AzStyledDom_new(body, AzCss_empty());
}

AzUpdate on_button_click(AzRefAny* data, AzCallbackInfo* info) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(data);
    if (!WidgetShowcase_downcastMut(data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->progress_value += 10.0;
    if (d.ptr->progress_value > 100.0) {
        d.ptr->progress_value = 0.0;
    }
    WidgetShowcaseRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_checkbox_toggle(AzRefAny* data, AzCallbackInfo* info) {
    WidgetShowcaseRefMut d = WidgetShowcaseRefMut_create(data);
    if (!WidgetShowcase_downcastMut(data, &d)) {
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
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Widget Showcase");
    window.state.size.dimensions.width = 600.0;
    window.state.size.dimensions.height = 500.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
