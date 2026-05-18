#include "azul.h"
#include <string.h>
#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
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
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) { return AzUpdate_DoNothing; }
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom unused_text = AzDom_createText(AZ_STR("X"));
    AzDom_delete(&unused_text);
    AzDom body = AzDom_createBody();
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
