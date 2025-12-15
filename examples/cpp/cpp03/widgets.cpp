// Widgets Showcase - C++03
// g++ -std=c++03 -o widgets widgets.cpp -lazul

#include <azul.hpp>
#include <string>

using namespace azul;

struct WidgetShowcase {
    bool enable_padding;
    size_t active_tab;
    float progress_value;
    bool checkbox_checked;
    std::string text_input;
};

void WidgetShowcase_init(WidgetShowcase* ws) {
    ws->enable_padding = true;
    ws->active_tab = 0;
    ws->progress_value = 25.0f;
    ws->checkbox_checked = false;
}

Update on_button_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    WidgetShowcase* d = WidgetShowcase_downcast_ref(data);
    if (!d) return StyledDom_default();
    
    // Create button
    Dom button = Dom_div();
    Dom_setInlineStyle(button, "
        margin-bottom: 10px; 
        padding: 10px; 
        background: #4CAF50;
    ");
    Dom_addChild(button, Dom_text("Click me!"));
    Dom_setCallback(button, On_MouseUp, RefAny_clone(data), on_button_click);

    // Create checkbox
    Dom checkbox = CheckBox_dom(CheckBox_new(d->checkbox_checked));
    Dom_setInlineStyle(checkbox, "margin-bottom: 10px;");

    // Create progress bar
    Dom progress = ProgressBar_dom(ProgressBar_new(d->progress_value));
    Dom_setInlineStyle(progress, "margin-bottom: 10px;");

    // Create text input
    TextInput ti = TextInput_new();
    TextInput_setPlaceholder(ti, "Enter text here...");
    Dom text_input = TextInput_dom(ti);
    Dom_setInlineStyle(text_input, "margin-bottom: 10px;");

    // Create color input
    ColorU color;
    color.r = 100; color.g = 150; color.b = 200; color.a = 255;
    Dom color_input = ColorInput_dom(ColorInput_new(color));
    Dom_setInlineStyle(color_input, "margin-bottom: 10px;");

    // Create number input
    Dom number_input = NumberInput_dom(NumberInput_new(42.0));
    Dom_setInlineStyle(number_input, "margin-bottom: 10px;");

    // Compose body
    Dom title = Dom_text("Widget Showcase");
    Dom_setInlineStyle(title, "font-size: 24px; margin-bottom: 20px;");

    Dom body = Dom_body();
    Dom_setInlineStyle(body, "padding: 20px; font-family: sans-serif;");
    Dom_addChild(body, title);
    Dom_addChild(body, button);
    Dom_addChild(body, checkbox);
    Dom_addChild(body, progress);
    Dom_addChild(body, text_input);
    Dom_addChild(body, color_input);
    Dom_addChild(body, number_input);

    return StyledDom_new(body, Css_empty());
}

Update on_button_click(RefAny& data, CallbackInfo& info) {
    WidgetShowcase* d = WidgetShowcase_downcast_mut(data);
    if (!d) return Update_DoNothing;
    d->progress_value += 10.0f;
    if (d->progress_value > 100.0f) {
        d->progress_value = 0.0f;
    }
    return Update_RefreshDom;
}

int main() {
    WidgetShowcase model;
    WidgetShowcase_init(&model);
    RefAny data = WidgetShowcase_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions_new(layout);
    WindowCreateOptions_setTitle(window, "Widget Showcase");
    WindowCreateOptions_setSize(window, LogicalSize_new(600, 500));
    
    App app = App_new(data, AppConfig_default());
    App_run(app, window);
    return 0;
}
