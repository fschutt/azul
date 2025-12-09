#include <azul.h>
#include <stdio.h>

typedef struct {
    uint32_t counter;
} MyDataModel;

void MyDataModel_delete(MyDataModel* restrict A) { }
AZ_REFLECT(MyDataModel, MyDataModel_delete);

// model -> view
AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutCallbackInfo* restrict info) {
    MyDataModelRef d = MyDataModelRef_create(data);
    if (!MyDataModel_downcastRef(data, &d)) {
        return AzStyledDom_empty(); // error
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzString const labelstring = AzString_copyFromBytes(buffer, 0, written);
    AzDom label = AzDom_text(labelstring);
    AzDom_setInlineStyle(&label, AzString_fromConstStr("font-size: 50px"));

    AzString const buttonstring = AzString_fromConstStr("Increase counter");
    AzDom button = AzDom_div();
    AzDom_setInlineStyle(&button, AzString_fromConstStr("flex-grow: 1"));

    AzDom body = AzDom_body();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    AzCss css = AzCss_empty();
    return AzStyledDom_new(body, css);
}

// model <- view
AzUpdate myOnClick(AzRefAny* restrict data, AzCallbackInfo* restrict info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(data);
    if (!MyDataModel_downcastRefMut(data, &d)) {
        return AzUpdate_DoNothing;
    }

    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);

    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(&model);
    AzAppConfig config = AzAppConfig_new(AzLayoutSolver_Default);
    AzApp app = AzApp_new(data, config);
    AzLayoutCallback layout_callback = AzLayoutCallback_new(myLayoutFunc);
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout_callback);
    AzApp_run(&app, window);
    return 0;
}
