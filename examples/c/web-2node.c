// Bisection: minimal 2-NODE direct-prop layout (body + 1 child div), simple
// constructor props only (NO struct literals, NO text, NO widget). web-direct-body.c
// (1 node, width+height) WORKS; flexbox-direct (5 nodes) + hello-world.c (multi-node)
// OOB in the cb. If THIS 2-node cb OOBs → the trigger is multi-node AzDom_addChild
// (tree-building mis-lift); if it RUNS → multi-node is fine, the trigger is prop
// count / struct-literal args / text.
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

static void add_prop(AzDom* d, AzCssProperty p) {
    AzDom_addCssProperty(d, AzCssPropertyWithConditions_simple(p));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;
    AzDom body = AzDom_createBody();
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(600.0f))));

    AzDom child = AzDom_createDiv();
    add_prop(&child, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(400.0f))));
    add_prop(&child, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(200.0f))));
    AzDom_addChild(&body, child);

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
