// Bisection: does a node with MANY inline props (large style-Vec grow, cap 256)
// corrupt the cascade — independent of the button's inner const-slice props?
// AzButton's container_style is a HEAP Vec of ~155 props (from_vec), grown to cap
// 256; flexbox-simple's nodes had <=16 props (cap 16, cascade OK). This div has 160
// SIMPLE inline values (NO const-slice inner Vecs, NO gradient/font-family), only
// the large-Vec-grow path. If THIS cascade-OOBs → the raw_vec grow/realloc/memcpy
// at high capacity is the button-corruption root. If it RUNS → the inner const-slice
// props (background StyleBackgroundContentVec / font-family StyleFontFamilyVec) are.
// The div is a PARENT (has a text child) so restyle iterates its 160 props.
//   cc -fno-stack-protector -o examples/c/web-manyprops.bin examples/c/web-manyprops.c -lazul -Ltarget/release -Idll
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

    // div with 160 simple inline props → style-Vec grows 1,2,4,...,256.
    AzDom div = AzDom_createDiv();
    for (int i = 0; i < 160; i++) {
        add_prop(&div, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px((float)(100 + i)))));
    }
    AzDom text = AzDom_createText(s("x"));   // child → div is a PARENT (restyle iterates its props)
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
