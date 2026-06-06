// Bisection: 2-LEVEL block nesting, NO flex. body > container > item, all block,
// width/height only. web-2node.c (body > div, 1-level) WORKS. This isolates whether
// the flexbox cb-OOB is the 2-level nesting (addChild to a node that itself has a
// child, then that subtree added to body) — if THIS OOBs too, nesting is the
// trigger; if it RUNS while web-flexbox-min OOBs, the display:flex/flex-grow props
// are. Keep ONLY to relift if flexbox-min still OOBs.
//
//   cc -o examples/c/web-nest2.bin examples/c/web-nest2.c -lazul -Ltarget/release -Idll
#include "azul.h"

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

    AzDom container = AzDom_createDiv();
    add_prop(&container, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(400.0f))));
    add_prop(&container, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(200.0f))));

    AzDom item = AzDom_createDiv();
    add_prop(&item, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(100.0f))));
    add_prop(&item, AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(50.0f))));

    AzDom_addChild(&container, item);   // 2nd-level: container gets a child
    AzDom_addChild(&body, container);   // then container (w/ child) → body
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
