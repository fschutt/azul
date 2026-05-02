// g++ -std=c++11 -o hello-world hello-world.cpp -lazul

#include "azul11.hpp"
#include <string>

using namespace azul;

// Data model: plain old struct - the "single source of truth" for app state.
// No AZ_REFLECT line in C++11+: reflection is template-based.
struct MyDataModel {
    uint32_t counter;
};

// All callbacks use the wrapper types - the codegen emits an extern "C"
// trampoline behind the scenes that adapts our azul::* signature to the
// raw Az* function pointer the framework dispatches through.
Update on_click(RefAny data, CallbackInfo info);

Dom layout(RefAny data, LayoutCallbackInfo info) {
    // azul::downcast_ref<T>(RefAny) -> const T* (or nullptr). Per-type
    // identity is derived from the address of a function-local static,
    // so the compiler stamps a unique tag per template instantiation.
    auto* d = downcast_ref<MyDataModel>(data);
    if (!d) return Dom::body();

    return Dom::body()
        .with_child(Dom::p_with_text(std::to_string(d->counter))
            .with_inline_style("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
            .with_button_type(ButtonType::Primary)
            .with_on_click(data.clone(), on_click)
            .dom())
        .style(Css::empty());
    // No .release(): returning a wrapper from a layout callback transparently
    // transfers ownership through the trampoline.
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
