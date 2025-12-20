// g++ -std=c++11 -o widgets widgets.cpp -lazul

#include "azul11.hpp"
#include <string>

using namespace azul;

struct WidgetShowcase {
    bool enable_padding;
    size_t active_tab;
    float progress_value;
    bool checkbox_checked;
    std::string text_input;
    
    WidgetShowcase() : enable_padding(true), active_tab(0), 
                       progress_value(25.0f), checkbox_checked(false) {}
};
AZ_REFLECT(WidgetShowcase);

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const WidgetShowcase* d = WidgetShowcase_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    // Create button
    Dom button_text = Dom::create_text("Click me!");
    Dom button = Dom::create_div();
    button.set_inline_style("margin-bottom: 10px; padding: 10px; background: #4CAF50; color: white;");
    button.add_child(std::move(button_text));
    button.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), data_wrapper.clone(), on_button_click);

    // Create title
    Dom title = Dom::create_text("Widget Showcase");
    title.set_inline_style("font-size: 24px; margin-bottom: 20px;");

    // Compose body
    Dom body = Dom::create_body();
    body.set_inline_style("padding: 20px; font-family: sans-serif;");
    body.add_child(std::move(title));
    body.add_child(std::move(button));

    return body.style(Css::empty()).release();
}

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    WidgetShowcase* d = WidgetShowcase_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->progress_value += 10.0f;
    if (d->progress_value > 100.0f) {
        d->progress_value = 0.0f;
    }
    return AzUpdate_RefreshDom;
}

int main() {
    WidgetShowcase model;
    RefAny data = WidgetShowcase_upcast(model);
    
    LayoutCallback cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(std::move(cb));
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Widget Showcase", 0, 15);
    window.inner().window_state.size.dimensions.width = 600.0;
    window.inner().window_state.size.dimensions.height = 500.0;
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
