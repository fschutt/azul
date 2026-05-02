// g++ -std=c++23 -o hello-world hello-world.cpp -lazul
// or, on a modules-aware toolchain:
//   g++ -std=c++23 -fmodules-ts -c azul.cppm
//   g++ -std=c++23 -fmodules-ts -o hello-world hello-world.cpp -lazul

#if __has_include(<azul.cppm>)
import std;
import azul;
#else
#include "azul23.hpp"
#include <expected>
#include <string>
#endif

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

Update on_click(RefAny data, CallbackInfo info);

Dom layout(RefAny data, LayoutCallbackInfo info) {
    auto* d = downcast_ref<MyDataModel>(data);
    if (!d) return Dom::body();

    // Result<Ok, Err> converts implicitly to std::expected<Ok, Err> in C++23 -
    // chain monadically with .and_then / .or_else.
    std::expected<Css, CssParseError> sheet = Css::parse(R"(
        body { background-color: #efefef; }
    )");
    Css css = std::move(sheet).value_or(Css::empty());

    return Dom::body()
        .with_child(Dom::p_with_text(std::to_string(d->counter))
            .with_inline_style("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
            .with_button_type(ButtonType::Primary)
            .with_on_click(data.clone(), on_click)
            .dom())
        .style(std::move(css));
    // Deducing-this in the wrapper means the same .with_* method works on
    // l-values and r-values without separate const&/&& overloads. The user
    // never sees this directly - it just keeps the chains above legal even
    // when 'css' is an l-value.
}

Update on_click(RefAny data, CallbackInfo info) {
    auto* d = downcast_mut<MyDataModel>(data);
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = upcast(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
