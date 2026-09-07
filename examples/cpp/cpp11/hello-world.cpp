#include "azul11.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

ffi::Update on_click(ffi::RefAny data, ffi::CallbackInfo info);

ffi::Dom layout(ffi::RefAny data, ffi::LayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return Dom::create_body();

    Dom label = Dom::create_p_with_text(String(std::to_string(d->counter)))
        .with_css(String("font-size: 32px;"));

    Button button = Button::create("Increase counter")
        .with_button_type(ButtonType::Primary)
        .with_on_click(data_wrapper.clone(), on_click);

    return Dom::create_body()
        .with_child(std::move(label))
        .with_child(button.dom());
}

ffi::Update on_click(ffi::RefAny data, ffi::CallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
