// Medium layout cb: adds a clickable Button (no snprintf, no string format).
// Used by the M9 5-step e2e demo — proves that
//   bootstrap → layout → hit-test → cb → patches
// works through pure-wasm.
#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);

AzJson MyDataModel_toJson(AzRefAny refany) {
    MyDataModelRef d = MyDataModelRef_create(&refany);
    int counter = 0;
    if (MyDataModel_downcastRef(&refany, &d)) {
        counter = (int)d.ptr->counter;
        MyDataModelRef_delete(&d);
    }
    return AzJson_int(counter);
}
AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionI64 counter_opt = AzJson_asInt(&json);
    int64_t value = (counter_opt.None.tag == AzOptionI64_Tag_None) ? 0 : counter_opt.Some.payload;
    MyDataModel model = { .counter = (uint32_t)value };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (MyDataModel_downcastMut(&data, &d)) {
        d.ptr->counter += 1;
        MyDataModelRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    return AzUpdate_DoNothing;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Static label (no snprintf)
    AzString label_text = AZ_STR("Counter");
    AzDom label = AzDom_createText(label_text);

    // Button
    AzButton button = AzButton_create(AZ_STR("Increase"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button_dom);
    return body;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 240.0;
    AzApp app = AzApp_create(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
