// Minimal layout cb for M9-2 probe diagnostics — no const strings,
// no snprintf, no AzString allocations. Just returns AzDom_createBody().
// Lets us isolate whether the trap is from user code (string ops) or
// from libazul's deeper deps.
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
    return AzJson_int(0);
}
AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionI64 counter_opt = AzJson_asInt(&json);
    int64_t value = (counter_opt.None.tag == AzOptionI64_Tag_None) ? 0 : counter_opt.Some.payload;
    MyDataModel model = { .counter = (uint32_t)value };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    return AzUpdate_DoNothing;
}

// Minimal layout: return an empty body. No string formatting, no Button.
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    return AzDom_createBody();
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
