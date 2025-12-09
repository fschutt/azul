#include <azul.h>
#include <stdio.h>

typedef struct {
    uint32_t counter;
} MyDataModel;

void MyDataModel_delete(void* A) { }
AZ_REFLECT(MyDataModel, MyDataModel_delete);

// model -> view
AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutCallbackInfo* restrict info) {
    MyDataModelRef d = MyDataModelRef_create(data);
    if (!MyDataModel_downcastRef(data, &d)) {
        return AzStyledDom_default(); // error
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzString const labelstring = AzString_copyFromBytes((const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_text(labelstring);
    AzString labelstyle = AzString_fromConstStr("font-size: 50px");
    AzDom_setInlineStyle(&label, labelstyle);

    AzString const buttonstring = AzString_fromConstStr("Increase counter");
    AzDom button = AzDom_div();
    AzString buttonstyle = AzString_fromConstStr("flex-grow: 1");
    AzDom_setInlineStyle(&button, buttonstyle);

    AzDom body = AzDom_body();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    AzCss css = AzCss_empty();
    return AzStyledDom_new(body, css);
}

// model <- view
AzUpdate myOnClick(AzRefAny* restrict data, AzCallbackInfo* restrict info) {
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
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_new(data, config);
    AzWindowCreateOptions window = AzWindowCreateOptions_new(myLayoutFunc);
    AzApp_run(&app, window);
    return 0;
}
