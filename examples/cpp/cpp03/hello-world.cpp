// g++ -std=c++03 -o hello-world hello-world.cpp -lazul

#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};
AZ_REFLECT(MyDataModel);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Callback must use C types for FFI compatibility
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const MyDataModel* d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    char buffer[32];
    std::snprintf(buffer, 32, "%u", d->counter);
    
    Dom label = Dom::create_text(String(buffer));
    label.set_inline_style(String("font-size: 50px;"));
    
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    Dom button_text = Dom::create_text(String("Increase counter"));
    Dom button = Dom::create_div();
    button.set_inline_style(String("flex-grow: 1;"));
    button.add_child(button_text);
    button.add_callback(event, data_wrapper.clone(), on_click);
    
    Dom body = Dom::create_body();
    body.add_child(label);
    body.add_child(button);
    
    return body.style(Css::empty()).release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    MyDataModel* d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model;
    model.counter = 5;
    RefAny data = MyDataModel_upcast(model);
    
    LayoutCallback layout_cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(layout_cb);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Hello World", 0, 11);
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
