// g++ -std=c++14 -o hello-world hello-world.cpp -lazul

#include "azul14.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// auto deduces the wrapper return type. The framework still sees AzDom
// because the trampoline at the wrapper boundary releases on return.
auto layout(AzRefAny data, AzLayoutCallbackInfo info) -> AzDom {
    RefAny data_wrapper(data);
    auto d = downcast_ref<MyDataModel>(data_wrapper);  // const MyDataModel*
    if (!d) return AzDom_createBody();

    return Dom::body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_inline_style("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom())
        .style(Css::empty())
        .release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = downcast_mut<MyDataModel>(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    // type_id_v is a variable template - shorthand for type_id<T>().
    static_assert(type_id_v<MyDataModel> != 0, "MyDataModel must have a type id");

    MyDataModel model = { 5 };
    RefAny data = upcast<MyDataModel>(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
