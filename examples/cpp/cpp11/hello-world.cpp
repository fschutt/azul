#include "azul11.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};
AZ_REFLECT(MyDataModel);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Callback must use C types for FFI compatibility
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    Dom label = Dom::create_text(String(std::to_string(d->counter).c_str()))
        .with_inline_style(String("font-size: 50px;"));
    
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    Dom button = Dom::create_div()
        .with_inline_style(String("flex-grow: 1;"))
        .with_child(Dom::create_text(String("Increase counter")))
        .with_callback(event, data_wrapper.clone(), on_click);
    
    Dom body = Dom::create_body()
        .with_child(std::move(label))
        .with_child(std::move(button));
    
    return body.style(Css::empty()).release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = {5};
    RefAny data = MyDataModel_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    
    return 0;
}
