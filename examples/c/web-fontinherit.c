// Discriminator: body{font-family} > text, with a HEAP font-family (fromItem, NOT
// const-slice). restyle inherits font-family body→text and clone_inheritable_property
// clones the StyleFontFamilyVec. The button cascade-OOBs cloning its font-family.
//   - If THIS OOBs → the font-family CLONE itself mis-lifts (StyleFontFamilyVec::clone /
//     its heap copy) — a general fix in clone_inheritable, independent of const-slice.
//   - If THIS RUNS → the clone is fine for a HEAP Vec ⇒ the button OOB is specifically
//     the CONST-SLICE font (from_const_slice MAC_FONT_FAMILY not mirrored) ⇒ fix the
//     transpiler data-mirror, not the clone.
//   cc -fno-stack-protector -o examples/c/web-fontinherit.bin examples/c/web-fontinherit.c -lazul -Ltarget/release -Idll
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
    // HEAP font-family (fromItem), inheritable → cloned in restyle's body→text inherit.
    AzStyleFontFamily ff = AzStyleFontFamily_system(s("serif"));
    AzStyleFontFamilyVec ffv = AzStyleFontFamilyVec_fromItem(ff);
    add_prop(&body, AzCssProperty_fontFamily(ffv));

    AzDom text = AzDom_createText(s("hello"));
    AzDom_addChild(&body, text);
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
