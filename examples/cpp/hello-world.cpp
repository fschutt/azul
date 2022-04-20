#include <azul.h>
#include <stdio.h>

struct MyDataModel {
    uint32_t counter;
};

// model -> view
StyledDom myLayoutFunc(RefAny* restrict data, LayoutInfo info) {
    auto d = data.createRef::<MyDataModel>(data);
    if !data.downcastRef(data, &d) {
        return StyledDom::empty(); // error
    }

    char buffer [20];
    int written = snprintf(buffer, 20, "%d", d->counter);

    auto const labelstring = String::copyFromBytes(&buffer, 0, written);
    auto label = Dom::text(labelstring);
    label.setInlineStyle(String::fromConstStr("font-size: 50px;"));

    auto const buttonstring = String::fromConstStr("Increase counter");
    auto button = Button::new(buttonstring)
    button.setOnClick(myOnClick, data.clone());
    auto button = button.dom();
    button.setInlineStyle(String::fromConstStr("flex-grow: 1;"));

    return Dom::body()
        .withChild(label)
        .withChild(button)
        .style(Css::empty());
}

Update myOnClick(RefAny* restrict data, CallbackInfo info) {
    auto d = data.createRefMut::<MyDataModel>(data);
    if !data.downcastRefMut(data, &d) {
        return Update::DoNothing; // error
    }

    d->counter += 1; // increase counter

    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzApp app = App_new(RefAny::new(model), AppConfig_default());
    app.run(WindowCreateOptions::new(myLayoutFunc));
    return 0;
}