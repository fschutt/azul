// g++ -std=c++03 -o hello-world hello-world.cpp -lazul

#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

AZ_REFLECT(MyDataModel)

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    azul::RefAny data_wrapper(data);
    const MyDataModel* d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return Dom::create_body().release();

    char buffer[20];
    std::snprintf(buffer, sizeof(buffer), "%u", d->counter);

    Dom label = Dom::create_div()
        .with_css(String("font-size: 32px;"))
        .with_child(Dom::create_text(String(buffer)));

    Button button = Button::create(String("Increase counter"))
        .with_button_type(ButtonType::Primary)
        .with_on_click(data_wrapper.clone(), on_click);

    return Dom::create_body()
        .with_child(label)
        .with_child(button.dom())
        .release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    (void)info;

    azul::RefAny data_wrapper(data);
    MyDataModel* d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model;
    model.counter = 5;
    azul::RefAny data = MyDataModel_upcast(model);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
