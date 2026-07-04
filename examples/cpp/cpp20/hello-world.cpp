// g++ -std=c++20 -o hello-world hello-world.cpp -lazul

#include "azul20.hpp"
#include <span>
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

static_assert(ReflectableModel<MyDataModel>);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

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
    if (!d) return Dom::create_body();

    return Dom::create_body()
        .with_child(Dom::create_div()
            .with_css("font-size: 32px;"sv)
            .with_child(Dom::create_text(String(std::to_string(d->counter).c_str()))))
        .with_child(Button::create("Increase counter"sv)
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom());
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
    RefAny data = RefAny::create(std::move(model));

    U8Vec bytes = U8Vec::from_item(0);
    (void)count_zero_bytes(bytes);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
