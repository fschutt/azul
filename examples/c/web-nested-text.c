// DIFFERENTIAL repro (2026-06-08): web-text-min.c (body>text, WORKS) but with the text wrapped in
// a DIV → body > div > text. This is hello-world's nesting MINUS the button. If this reproduces
// content.len=0 (nested text never positions) → the bug is the NESTING (force_ifc IFC-on-a-div
// path), and THIS is a minimal repro. If it POSITIONS like web-text-min → the bug is the BUTTON,
// not the nesting. No counter/snprintf/click — pure layout isolation.
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

    AzString s = AzString_copyFromBytes((const uint8_t*)"Hello", 0, 5);
    AzDom text = AzDom_createText(s);

    // The ONLY difference vs web-text-min: wrap the text in a DIV (forces the nested
    // body(BFC) > div(IFC) > text path that hello-world's label_wrapper takes).
    AzDom div = AzDom_createDiv();
    AzDom_addChild(&div, text);
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
