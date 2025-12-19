// g++ -std=c++14 -o widgets widgets.cpp -lazul

#include <azul.hpp>
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

Update on_button_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    WidgetShowcase* d = WidgetShowcase::downcast_ref(data);
    if (!d) return StyledDom::default_value();
    
    // Create button
    Dom button = Dom::create_div()
        .with_inline_style("margin-bottom: 10px; padding: 10px; background: #4CAF50; color: white;")
        .with_child(Dom::create_text("Click me!"))
        .with_callback(On::MouseUp, data.clone(), on_button_click);

    // Create checkbox
    Dom checkbox = CheckBox::create(d->checkbox_checked)
        .dom()
        .with_inline_style("margin-bottom: 10px;");

    // Create progress bar
    Dom progress = ProgressBar::create(d->progress_value)
        .dom()
        .with_inline_style("margin-bottom: 10px;");

    // Create text input
    Dom text_input = TextInput::create()
        .with_placeholder("Enter text here...")
        .dom()
        .with_inline_style("margin-bottom: 10px;");

    // Create color input
    ColorU color = {100, 150, 200, 255};
    Dom color_input = ColorInput::create(color)
        .dom()
        .with_inline_style("margin-bottom: 10px;");

    // Create number input
    Dom number_input = NumberInput::create(42.0)
        .dom()
        .with_inline_style("margin-bottom: 10px;");

    // Compose body
    Dom title = Dom::create_text("Widget Showcase")
        .with_inline_style("font-size: 24px; margin-bottom: 20px;");

    Dom body = Dom::create_body()
        .with_inline_style("padding: 20px; font-family: sans-serif;")
        .with_child(title)
        .with_child(button)
        .with_child(checkbox)
        .with_child(progress)
        .with_child(text_input)
        .with_child(color_input)
        .with_child(number_input);

    return body.style(Css::empty());
}

Update on_button_click(RefAny& data, CallbackInfo& info) {
    WidgetShowcase* d = WidgetShowcase::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->progress_value += 10.0f;
    if (d->progress_value > 100.0f) {
        d->progress_value = 0.0f;
    }
    return Update::RefreshDom;
}

int main() {
    WidgetShowcase model;
    RefAny data = RefAny::create(model);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.set_title("Widget Showcase");
    window.set_size(LogicalSize(600, 500));
    
    App app = App::create(data, AppConfig::default_value());
    app.run(window);
    return 0;
}
