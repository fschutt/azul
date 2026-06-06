// Bisection probe: the SIMPLEST possible layout — a single sized <body> (1 node,
// NO flex, NO children). If the lifted solveLayoutReal still Errs/0-rects on this,
// the bug is in basic block sizing (fundamental); if it succeeds, flex/children is
// the trigger. Mirrors web-flexbox-simple.c minus the container/items.
#include "azul.h"
#include <string.h>

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { (void)m; }
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);
AzJson MyDataModel_toJson(AzRefAny refany) { (void)refany; return AzJson_int(0); }
AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    (void)json;
    MyDataModel m = { .counter = 0 };
    return AzResultRefAnyString_ok(MyDataModel_upcast(m));
}

static AzString s(const char* c) {
    return AzString_copyFromBytes((const uint8_t*)c, 0, strlen(c));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;
    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, s("box-sizing:border-box; margin:0; padding:20px; width:800px; height:600px;"));
    return body;
}

int main(void) {
    MyDataModel m = { .counter = 0 };
    AzRefAny d = MyDataModel_upcast(m);
    AzWindowCreateOptions w = AzWindowCreateOptions_create(layout);
    AzApp a = AzApp_create(d, AzAppConfig_default());
    AzApp_run(&a, w);
    return 0;
}
