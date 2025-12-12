// Widgets Showcase - C++20
// g++ -std=c++20 -o widgets widgets.cpp -lazul

#include <azul.hpp>
#include <format>

using namespace azul;
using namespace std::string_view_literals;

struct WidgetShowcase {
    bool enable_padding = true;
    size_t active_tab = 0;
    float progress_value = 25.0f;
    bool checkbox_checked = false;
    std::string text_input;
};

Update on_button_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = WidgetShowcase::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    // Create button
    auto button = Dom::div()
        .with_inline_style("margin-bottom: 10px; padding: 10px; background: #4CAF50; color: white; cursor: pointer;"sv)
        .with_child(Dom::text("Click me!"sv))
        .with_callback(On::MouseUp, data.clone(), on_button_click);

    // Create checkbox
    auto checkbox = CheckBox::new(d->checkbox_checked)
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Create progress bar
    auto progress = ProgressBar::new(d->progress_value)
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Create text input
    auto text_input = TextInput::new()
        .with_placeholder("Enter text here..."sv)
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Create color input
    auto color_input = ColorInput::new(ColorU{100, 150, 200, 255})
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Create number input
    auto number_input = NumberInput::new(42.0)
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Create dropdown
    auto dropdown = DropDown::new({"Option 1"sv, "Option 2"sv, "Option 3"sv})
        .dom()
        .with_inline_style("margin-bottom: 10px;"sv);

    // Compose body
    auto title = Dom::text("Widget Showcase"sv)
        .with_inline_style("font-size: 24px; margin-bottom: 20px;"sv);

    auto body = Dom::body()
        .with_inline_style("padding: 20px; font-family: sans-serif;"sv)
        .with_child(title)
        .with_child(button)
        .with_child(checkbox)
        .with_child(progress)
        .with_child(text_input)
        .with_child(color_input)
        .with_child(number_input)
        .with_child(dropdown);

    return body.style(Css::empty());
}

Update on_button_click(RefAny& data, CallbackInfo& info) {
    auto d = WidgetShowcase::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->progress_value += 10.0f;
    if (d->progress_value > 100.0f) {
        d->progress_value = 0.0f;
    }
    return Update::RefreshDom;
}

int main() {
    auto data = RefAny::new(WidgetShowcase{});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Widget Showcase"sv);
    window.set_size(LogicalSize(600, 500));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
