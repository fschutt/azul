#include "azul.h"
#include <string.h>
typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);
AzJson MyDataModel_toJson(AzRefAny refany) { return AzJson_int(0); }
AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    MyDataModel m = { .counter = 5 };
    return AzResultRefAnyString_ok(MyDataModel_upcast(m));
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
    AzDom body = AzDom_createBody();
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzEventFilter filter = AzEventFilter_hover(AzHoverEventFilter_MouseUp);
    AzDom_addCallback(&body, filter, data_clone, on_click);
    return body;
}
int main() {
    MyDataModel m = {.counter=5};
    AzRefAny d = MyDataModel_upcast(m);
    AzWindowCreateOptions w = AzWindowCreateOptions_create(layout);
    AzApp a = AzApp_create(d, AzAppConfig_default());
    AzApp_run(&a, w);
    return 0;
}
