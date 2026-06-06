// EXACT confirmation: a div with ONE on_hover prop (non-empty const-slice apply_if =
// from_const_slice(&[PseudoState(Hover)]), the SAME path AzButton uses). restyle's
// condition filter (prop_cache.rs:1182, runs BEFORE the inheritable check) reads
// conds.as_slice() + conditions.iter() on EVERY prop → derefs this onHover prop's const
// [Hover] slice. The div is a PARENT (text child) so restyle iterates its props.
//   - OOBs → CONFIRMED: non-empty const-slice apply_if conditions are the button root
//     (fix = transpiler data-mirror of the [Hover]/[Active] DynamicSelector const slices).
//   - RUNS → the apply_if-condition hypothesis is ALSO wrong; the button OOB is something
//     else (nested widget / callbacks / ids-classes).
//   cc -fno-stack-protector -o examples/c/web-hovercond.bin examples/c/web-hovercond.c -lazul -Ltarget/release -Idll
#include "azul.h"
#include <string.h>

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { (void)m; }
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);
AzJson MyDataModel_toJson(AzRefAny refany) { (void)refany; return AzJson_int(0); }
AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    (void)json; MyDataModel m = { .counter = 0 };
    return AzResultRefAnyString_ok(MyDataModel_upcast(m));
}
static void add_prop(AzDom* d, AzCssProperty p) {
    AzDom_addCssProperty(d, AzCssPropertyWithConditions_simple(p));
}
static AzString s(const char* c) { return AzString_copyFromBytes((const uint8_t*)c, 0, strlen(c)); }

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;
    AzDom body = AzDom_createBody();
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(600.0f))));

    AzDom div = AzDom_createDiv();
    add_prop(&div, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(400.0f))));
    // The ON_HOVER prop → non-empty const-slice [Hover] apply_if (button's exact path).
    AzDom_addCssProperty(&div, AzCssPropertyWithConditions_onHover(
        AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(500.0f)))
    ));
    AzDom text = AzDom_createText(s("x"));
    AzDom_addChild(&div, text);   // div is a PARENT → restyle iterates its props
    AzDom_addChild(&body, div);
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
