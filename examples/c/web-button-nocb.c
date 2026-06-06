// Bisection: AzButton WITHOUT setOnClick (no callback node). web-button.c (with
// on_click) ran the cb but OOB'd in the CASCADE. If THIS (no callback) RUNS → the
// on_click CALLBACK-node registration is the cascade-OOB trigger; if it OOBs → the
// button widget's DOM/styling itself.
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
    // DIAG (2026-06-02): explicit body font-size (inheritable → button label). If the label
    // text height goes 0 → >0, the original 0 was a missing/zero font-size (UA/cascade), not
    // allsorts shaping. The label has no font-size of its own (probe 2 stripped label_style).
    add_prop(&body, AzCssProperty_fontSize(AzStyleFontSize_px(16.0f)));
    // DIAG (2026-06-02): explicit body font-family serif (inheritable → label). Tests whether
    // the text-0 is a font-CHAIN-resolves-0 issue (default font-family not collected/resolved)
    // vs allsorts shaping. serif → DEFAULT_FONT_ID → the embedded SourceSerifPro (with_memory_fonts).
    {
        AzStyleFontFamily ff = AzStyleFontFamily_system(AZ_STR("serif"));
        AzStyleFontFamilyVec ffv = AzStyleFontFamilyVec_fromItem(ff);
        add_prop(&body, AzCssProperty_fontFamily(ffv));
    }

    AzButton button = AzButton_create(AZ_STR("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    // NO setOnClick — isolates the widget DOM from the callback node.
    AzDom button_dom = AzButton_dom(button);
    AzDom_addChild(&body, button_dom);

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
