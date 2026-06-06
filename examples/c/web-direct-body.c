// Direct-props test: a <body> sized via AzDom_addCssProperty (numeric CssProperty
// structs), NOT via AzDom_setCss string parsing. This sidesteps the lifted CSS
// string parser (Css::parse_inline → 0 rules in the lift) AND the user-binary
// __cstring data-section mirror. If the lifted layout reports body=(0,0,800,600)
// — h=600 is the tell (only achievable from CSS height, not viewport-fill) — then
// the cascade + getters + solver read direct props correctly, and the only gap is
// the string path. If h=0, the getters niche-read the props as auto in the lift.
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

// Add a single inline CSS property to a node (no string parsing).
static void add_prop(AzDom* dom, AzCssProperty prop) {
    AzDom_addCssProperty(dom, AzCssPropertyWithConditions_simple(prop));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;
    AzDom body = AzDom_createBody();
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(600.0f))));
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
