// g++ -std=c++14 -o hello-world hello-world.cpp -lazul

#include "azul14.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

auto layout(AzRefAny data, AzLayoutCallbackInfo info) -> AzDom {
    RefAny data_wrapper(data);
    auto d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return Dom::create_body().release();

    return Dom::create_body()
        .with_child(Dom::create_div()
            .with_css(String("font-size: 32px;"))
            .with_child(Dom::create_text(String(std::to_string(d->counter).c_str()))))
        .with_child(Button::create("Increase counter")
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom());
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    // type_id_v is a variable template - shorthand for RefAny::type_id<T>().
    // The address-of-static trick that backs it isn't a constant expression,
    // so we can't static_assert; just verify at runtime.
    if (RefAny::type_id_v<MyDataModel> == 0) return 1;

    MyDataModel model = { 5 };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
