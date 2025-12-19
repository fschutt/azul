struct MyDataModel {
    uint32_t counter;
};

void MyDataModel_destructor(MyDataModel*) { }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

Update on_click(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    MyDataModelRef d = MyDataModelRef::create(data);
    if (!MyDataModel::downcastRef(data, d)) {
        return StyledDom::default();
    }
    
    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d->counter);
    
    Dom label = Dom::create_text(String(buffer, written))
        .with_inline_style("font-size: 50px;");
    
    Dom button = Dom::create_div()
        .with_inline_style("flex-grow: 1;")
        .with_child(Dom::create_text("Increase counter"))
        .with_callback(On::MouseUp, data.clone(), on_click);
    
    Dom body = Dom::create_body()
        .with_child(label)
        .with_child(button);
    
    return body.style(Css::empty());
}

Update on_click(RefAny& data, CallbackInfo& info) {
    MyDataModelRefMut d = MyDataModelRefMut::create(data);
    if (!MyDataModel::downcastMut(data, d)) {
        return Update::DoNothing;
    }
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = MyDataModel::upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("Hello World");
    window.set_size(LogicalSize(400, 300));
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
    
    return 0;
}
