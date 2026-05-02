// g++ -std=c++11 -o hello-world hello-world.cpp -lazul

#include "azul11.hpp"
#include <string>

using namespace azul;

// Data model: plain old struct - the "single source of truth" for app state.
// No AZ_REFLECT line in C++11+: reflection is template-based.
struct MyDataModel {
    uint32_t counter;
};

// Callback signatures stay on the raw Az* types because the framework
// dispatches through C function pointers. The body adopts the raw handle
// into RAII wrappers immediately.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);

    // azul::downcast_ref<T>(RefAny&) -> const T* (or nullptr). Per-type
    // identity is derived from the address of a template-instantiated
    // static, so the compiler stamps a unique tag per T at link time.
    auto* d = downcast_ref<MyDataModel>(data_wrapper);
    if (!d) return Dom::create_body().release();

    Dom label = Dom::p_with_text(String(std::to_string(d->counter).c_str()))
        .with_css(String("font-size: 50px;"));

    Button button = Button::create("Increase counter")
        .with_button_type(AzButtonType_Primary)
        .with_on_click(data_wrapper.clone(), on_click);

    return Dom::create_body()
        .with_child(std::move(label))
        .with_child(button.dom())
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
