#include <azul.h>
#include <stdio.h>

struct MyDataModel {
    uint32_t counter;
};

static css = String::fromConstStr(
    ".__azul-native-label { font-size: 50px; }"
);

// model -> view
StyledDom myLayoutFunc(RefAny* restrict data, LayoutInfo info) {
    auto d = data.createRef::<MyDataModel>(data);
    if !data.downcastRef(data, &d) {
        return StyledDom::empty(); // error
    }

    char buffer [20];
    int written = snprintf(buffer, 20, "%d", d->counter);

    auto const labelstring = String::copyFromBytes(&buffer, 0, written);
    auto const label = Label::new(labelstring);

    auto const buttonstring = String::fromConstStr("Increase counter");
    auto const button = Button::new(buttonstring)
        .withOnClick(myOnClick, data.clone());

    return Dom::body()
        .withChild(label.dom())
        .withChild(button.dom())
        .style(Css::fromString(css));
}

Update myOnClick(RefAny* restrict data, CallbackInfo info) {
    auto d = data.createRefMut::<MyDataModel>(data);
    if !data.downcastRefMut(data, &d) {
        return Update::DoNothing; // error
    }

    d->counter += 1; // increase counter

    // tell azul to call the myLayoutFunc again
    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzApp app = App_new(RefAny::new(model), AppConfig_default());
    app.run(WindowCreateOptions::new(myLayoutFunc));
    return 0;
}