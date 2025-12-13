using namespace azul;
using namespace std::string_view_literals;

// No AZ_REFLECT needed in C++17+
struct MyDataModel { uint32_t counter; };

Update on_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = MyDataModel::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto label = Dom::text(std::format("{}", d->counter))
        .with_inline_style("font-size: 50px;"sv);
    
    auto button = Dom::div()
        .with_inline_style("flex-grow: 1;"sv)
        .with_child(Dom::text("Increase counter"sv))
        .with_callback(On::MouseUp, data.clone(), on_click);
    
    auto body = Dom::body()
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
    auto data = RefAny::new(MyDataModel{.counter = 5});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Hello World"sv);
    window.set_size(LogicalSize(400, 300));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
