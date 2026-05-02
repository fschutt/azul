// g++ -std=c++20 -o hello-world hello-world.cpp -lazul
//
// On a modules-aware toolchain you can replace the include below with
// `import azul;` after precompiling the sibling `azul.cppm`:
//   clang++ -std=c++20 -fmodules -c azul.cppm
//   clang++ -std=c++20 -fmodules -o hello-world hello-world.cpp -lazul

#include "azul20.hpp"
#include <span>
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

// The ReflectableModel concept constrains upcast / downcast_ref / downcast_mut /
// type_id - feeding a non-reflectable type produces a readable requires-clause
// error rather than a wall of template-instantiation noise. Concept satisfied
// here at compile time so the static_assert is a real check.
static_assert(ReflectableModel<MyDataModel>);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Every Vec wrapper exposes a zero-copy `toSpan()` and an implicit conversion
// to `std::span` - hand it straight to a stdlib algorithm without an extra copy.
static size_t count_zero_bytes(std::span<const uint8_t> bytes) {
    size_t n = 0;
    for (auto b : bytes) {
        if (b == 0) ++n;
    }
    return n;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return AzDom_createBody();

    return Dom::create_body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_css("font-size: 50px;"sv))
        .with_child(Button::create("Increase counter"sv)
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom())
        .style(Css::empty())
        .release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = RefAny::create<MyDataModel>(std::move(model));

    // Build a small Vec and view it as std::span without a copy.
    U8Vec bytes = U8Vec::from_item(0);
    (void)count_zero_bytes(bytes);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
