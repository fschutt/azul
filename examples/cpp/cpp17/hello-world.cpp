// g++ -std=c++17 -o hello-world hello-world.cpp -lazul

#include "azul17.hpp"
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_ref<MyDataModel>(data_wrapper);
    if (!d) return AzDom_createBody();

    return Dom::body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_inline_style("font-size: 50px;"sv))
        .with_child(Button::create("Increase counter"sv)
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom())
        .style(Css::empty())
        .release();
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

// C++17 also gives us structured bindings on Result<Ok, Err>:
//
//     auto result = azul::xml::parse(src);
//     if (auto [ok, err] = std::move(result); ok) { /* use *ok */ }
//
// and Option<T>::toStdOptional() -> std::optional<T> for natural
// interop with the standard library. Our counter doesn't surface either,
// but they're available on every Result/Option the codegen emits.
