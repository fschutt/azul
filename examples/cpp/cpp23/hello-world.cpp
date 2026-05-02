// g++ -std=c++23 -o hello-world hello-world.cpp -lazul
//
// On a modules-aware toolchain you can replace the includes below with
// `import std; import azul;` after precompiling the sibling `azul.cppm`:
//   clang++ -std=c++23 -fmodules -c azul.cppm
//   clang++ -std=c++23 -fmodules -o hello-world hello-world.cpp -lazul

#include "azul23.hpp"
#include <expected>
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_ref<MyDataModel>(data_wrapper);
    if (!d) return AzDom_createBody();

    // Css::from_string returns Css directly (no error path) — for a Result-
    // typed example, see docs on `Url::parse`, which yields a wrapper that
    // converts to std::expected via `toStdExpected()` (and the matching
    // implicit conversion).
    Css css = Css::from_string(String(R"(body { background-color: #efefef; })"));

    return Dom::create_body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_css("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom())
        .style(std::move(css))
        .release();
    // Deducing-this in the wrapper means the same .with_* method works on
    // l-values and r-values without separate const&/&& overloads. The user
    // never sees this directly - it just keeps the chains above legal even
    // when 'css' is an l-value.
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_mut<MyDataModel>(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = upcast<MyDataModel>(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
