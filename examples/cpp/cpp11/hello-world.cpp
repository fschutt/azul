// Hello World with Button - C++11
// g++ -std=c++11 -o hello-world hello-world.cpp -lazul

#include <azul.hpp>
#include <string>
using namespace azul;

struct MyDataModel {
    uint32_t counter;
};
AZ_REFLECT(MyDataModel);

Update on_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = MyDataModel::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    Dom label = Dom::text(std::to_string(d->counter))
        .with_inline_style("font-size: 50px;");
    
    Dom button = Dom::div()
        .with_inline_style("flex-grow: 1;")
        .with_child(Dom::text("Increase counter"))
        .with_callback(On::MouseUp, data.clone(), on_click);
    
    Dom body = Dom::body()
        .with_child(label)
        .with_child(button);
    
    return body.style(Css::empty());
}

Update on_click(RefAny& data, CallbackInfo& info) {
    auto d = MyDataModel::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model{5};
    RefAny data = RefAny::new(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("Hello World");
    window.set_size(LogicalSize(400, 300));
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
}
