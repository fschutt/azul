// g++ -std=c++20 -o hello-world hello-world.cpp -lazul
//
// On a modules-aware toolchain you can replace the include below with
// `import azul;` after precompiling the sibling `azul.cppm`:
//   clang++ -std=c++20 -fmodules -c azul.cppm
//   clang++ -std=c++20 -fmodules -o hello-world hello-world.cpp -lazul

#include "azul20.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

// The ReflectableModel concept constrains downcast_ref/downcast_mut/upcast,
// so feeding a non-reflectable type to one of those produces a readable
// requires-clause error rather than a wall of template-instantiation noise.
template<ReflectableModel T>
constexpr bool is_reflectable_v = true;
static_assert(is_reflectable_v<MyDataModel>);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_ref<MyDataModel>(data_wrapper);
    if (!d) return AzDom_createBody();

    return Dom::create_body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_css("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
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

    // Designated initializers on the in-header builder helpers - emitted by
    // the codegen for POD-shaped option structs (window state, app config).
    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
