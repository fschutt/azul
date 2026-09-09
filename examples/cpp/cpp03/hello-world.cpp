#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

AZ_REFLECT(MyDataModel)

ffi::Update on_click(ffi::RefAny data, ffi::CallbackInfo info);

ffi::Dom layout(ffi::RefAny data, ffi::LayoutCallbackInfo info) {
    (void)info;

    RefAny data_wrapper(data);
    const MyDataModel* d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return Dom::create_body().release();

    char buffer[20];
    std::snprintf(buffer, sizeof(buffer), "%u", d->counter);

    Dom label = Dom::create_p_with_text(String(buffer))
        .with_css(String("font-size: 32px; margin: 0;"));

    Button button = Button::create(String("Increase counter"))
        .with_button_type(ButtonType::Primary)
        .with_on_click(data_wrapper.clone(), on_click);

    return Dom::create_body()
        .with_child(label)
        .with_child(button.dom())
        .release();
}

ffi::Update on_click(ffi::RefAny data, ffi::CallbackInfo info) {
    (void)info;

    RefAny data_wrapper(data);
    MyDataModel* d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model;
    model.counter = 5;
    RefAny data = MyDataModel_upcast(model);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
