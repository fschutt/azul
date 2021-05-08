#include <azul.h>
#include <stdio.h>

typedef struct {
    uint32_t counter;
} MyDataModel;

void DataModel_delete(MyDataModel* restrict A) { }
AZ_REFLECT(MyDataModel, MyDataModel_delete);

AzString css = AzString_fromConstStr("
    .__azul-native-label { font-size: 50px; }
");

// model -> view
AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutInfo info) {
    MyDataModelRef d = MyDataModelRef_create(data);
    if !(DataModel_downcastRef(data, &d)) {
        return AzStyledDom_empty(); // error
    }

    char buffer [20];
    int written = snprintf(buffer, 20, "%d", d->counter);

    AzString const labelstring = AzString_copyFromBytes(&buffer, 0, written);
    AzLabel const label = AzLabel_new(labelstring);

    AzString const buttonstring = AzString_fromConstStr("Increase counter");
    AzButton button = AzButton_new(buttonstring, AzRefAny_clone(data));
    AzButton_setOnClick(&button, myOnClick);

    AzDom body = AzDom_body();
    AzDom_addChild(&body, AzLabel_dom(label));
    AzDom_addChild(&body, AzButton_dom(button));

    MyDataModelRef_delete(&d);

    return AzStyledDom_new(html, AzCss_fromString(css));
}

// model <- view
AzUpdate myOnClick(AzRefAny* restrict data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(data);
    if !(DataModel_downcastRefMut(data, &d)) {
        return AzUpdate_DoNothing; // error
    }
    // increase counter
    d->counter += 1;
    MyDataModelRefMut_delete(&d);

    // tell azul to call the myLayoutFunc again
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny upcasted = MyDataModel_upcast(model);
    AzApp app = AzApp_new(upcasted, AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(myLayoutFunc));
    return 0;
}