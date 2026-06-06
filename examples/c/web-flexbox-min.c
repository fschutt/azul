// MINIMAL multi-node flex (bisection): body + flex container + 3 flex-grow items,
// ONLY width/height/display/flex-grow props — NO border (color structs), NO
// box-sizing, NO padding. web-flexbox-simple.c (full props) OOBs in the LIFTED
// layout cb during DOM construction (initLayoutCache → function[6] OOB). This
// isolates whether the OOB is the nesting+flex CORE (then THIS OOBs too) or the
// complex props border/box-sizing/padding (then THIS RUNS and proves the flex-grow
// 1:2:3 split). All props use the if-let-safe path in apply (Width/Height/Display/
// FlexGrow), avoiding apply_css_property_to_compact's jump-table match.
//
//   cc -o examples/c/web-flexbox-min.bin examples/c/web-flexbox-min.c -lazul -Ltarget/release -Idll
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

    // body: width:800px; height:600px;  (no padding/box-sizing)
    AzDom body = AzDom_createBody();
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(600.0f))));

    // container: display:flex; width:800px; height:100px;  (no border/box-sizing)
    AzDom container = AzDom_createDiv();
    add_prop(&container, AzCssProperty_display(AzLayoutDisplay_flex()));
    add_prop(&container, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(800.0f))));
    add_prop(&container, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(100.0f))));

    // 3 flex items: flex-grow:1/2/3 (no border/box-sizing) → split 800 → ~133/267/400
    for (int i = 0; i < 3; i++) {
        AzDom item = AzDom_createDiv();
        add_prop(&item, AzCssProperty_flexGrow(AzLayoutFlexGrow_create((float)(i + 1))));
        AzDom_addChild(&container, item);
    }

    AzDom_addChild(&body, container);
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
