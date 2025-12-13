#include <azul.h>
#include <stdio.h>

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(MyDataModel* m) { }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

static const AzString LABEL_STYLE = AzString_fromConstStr("font-size:50px;");
static const AzString BUTTON_STYLE = AzString_fromConstStr("flex-grow:1;");
static const AzString BUTTON_TEXT = AzString_fromConstStr("Increase counter");

AzUpdate on_click(AzRefAny* data, AzCallbackInfo* info);

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    MyDataModelRef d = MyDataModelRef_create(data);
    if (!MyDataModel_downcastRef(data, &d)) {
        return AzStyledDom_default();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzDom label = AzDom_text(AzString_copyFromBytes((uint8_t*)buffer, 0, written));
    AzDom_setInlineStyle(&label, LABEL_STYLE);

    AzDom button = AzDom_div();
    AzDom_setInlineStyle(&button, BUTTON_STYLE);
    AzDom_addChild(&button, AzDom_text(BUTTON_TEXT));
    AzDom_setCallback(&button, AzOn_MouseUp, AzRefAny_clone(data), on_click);

    AzDom body = AzDom_body();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    return AzStyledDom_new(body, AzCss_empty());
}

AzUpdate on_click(AzRefAny* data, AzCallbackInfo* info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(data);
    if (!MyDataModel_downcastMut(data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Hello World");
    window.state.size.dimensions.width = 400.0;
    window.state.size.dimensions.height = 300.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
