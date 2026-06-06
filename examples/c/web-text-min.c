// Minimal body + TEXT node (no AzButton widget) — isolates the font→text-measure path from
// the button-DOM lift-fragility. If with_memory_fonts does NOT trap here AND the "Hello"
// text node measures a non-zero height with real metrics (upem=1000), the JS-font + web_lift
// + rust-fontconfig fork chain is validated end-to-end for text measurement on the web lift.
#include "azul.h"
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

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
    add_prop(&body, AzCssProperty_fontSize(AzStyleFontSize_px(16.0f)));

    /* [az-diag REVERT] capture the AzString that AzString_copyFromBytes returns, BEFORE
       createText, to split copy_from_bytes-bug from createText-bug. Gated native-safe: in
       wasm the "Hello" literal is the LOW mirrored addr (<16MB); natively it's a HIGH real
       addr so the marker writes are SKIPPED (no wild native write / crash). */
    const char* az_hp = "Hello";
    /* len HARDCODED 5 (NOT strlen — a runtime strlen(variable) may stub to 0; the original
       AZ_STR("Hello") folds strlen at compile time to 5). */
    AzString s = AzString_copyFromBytes((const uint8_t*)az_hp, 0, 5);
    if ((unsigned long)(const void*)az_hp < 0x1000000UL) {
        const unsigned int* sp = (const unsigned int*)(const void*)&s;
        *(volatile unsigned int*)0x40760 = 0x0000C0DEU; /* cb ran in WASM + layout markers WORK */
        *(volatile unsigned int*)0x40764 = sp[0];       /* AzString.ptr_lo (expect heap ~0x6xxxxxx) */
        *(volatile unsigned int*)0x40768 = sp[2];       /* AzString.len_lo (expect 5) */
    }
    AzDom text = AzDom_createText(s);
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
