// g++ -std=c++03 -o widgets widgets.cpp -lazul

#include "azul03.hpp"

using namespace azul;

struct WidgetShowcase {
    float progress_value;
};
AZ_REFLECT(WidgetShowcase);

AzUpdate on_button_click(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const WidgetShowcase* d = WidgetShowcase_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    Dom button_text = Dom::create_text(String("Click me!"));
    Dom button = Dom::create_div();
    button.set_inline_style(String("margin-bottom: 10px; padding: 10px; background: #4CAF50; color: white;"));
    button.add_child(button_text);
    button.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), data_wrapper.clone(), on_button_click);

    Dom title = Dom::create_text(String("Widget Showcase"));
    title.set_inline_style(String("font-size: 24px; margin-bottom: 20px;"));

    Dom body = Dom::create_body();
    body.set_inline_style(String("padding: 20px; font-family: sans-serif;"));
    body.add_child(title);
    body.add_child(button);

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
    model.progress_value = 25.0f;
    RefAny data = WidgetShowcase_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Widget Showcase", 0, 15);
    window.inner().window_state.size.dimensions.width = 600.0;
    window.inner().window_state.size.dimensions.height = 500.0;
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
