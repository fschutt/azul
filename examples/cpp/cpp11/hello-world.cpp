#include "azul11.hpp"
using namespace azul;

struct MyDataModel {
    uint32_t counter;
};
AZ_REFLECT(MyDataModel);

Update on_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = MyDataModel_downcast_ref(data);
    if (!d) return StyledDom::default_();
    
    Dom label = Dom::create_text(String(std::to_string(d->counter).c_str()))
        .with_inline_style(String("font-size: 50px;"));
    
    Dom button = Dom::create_div()
        .with_inline_style(String("flex-grow: 1;"))
        .with_child(Dom::create_text(String("Increase counter")))
        .with_callback(On_MouseUp, data.clone(), on_click);
    
    Dom body = Dom::create_body()
        .with_child(label)
        .with_child(button);
    
    return body.style(Css::empty());
}

Update on_click(RefAny& data, CallbackInfo& info) {
    auto d = MyDataModel_downcast_mut(data);
    if (!d) return Update_DoNothing;
    d->counter += 1;
    return Update_RefreshDom;
}

int main() {
    MyDataModel model = {5};
    RefAny data = MyDataModel_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::new_(LayoutCallback::new_(layout));
    window.set_title(String("Hello World"));
    window.set_size(LogicalSize::new_(400, 300));
    
    App app = App::new_(data, AppConfig::default_());
    app.run(window);
}
