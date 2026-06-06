// Web layout-correctness example: flexbox-simple (pure boxes, NO fonts/text).
//
// Builds the `flexbox-simple` reftest (doc/working/flexbox-simple.xht) as a Dom
// via DIRECT CSS-property constructors (AzCssProperty_*), NOT AzDom_setCss string
// parsing: the lifted CSS string parser (Css::parse_inline) mis-lifts to "0 rules"
// in wasm, so the web example must set props directly (hello-world.c does the same
// via AzDom_addCssProperty). The CSS VALUES match layout/tests/web_flexbox_simple_ref.rs
// exactly (box-sizing:border-box; body padding:20px width:800px height:600px;
// container display:flex width:100% height:100px border:5px; items flex-grow:1/2/3
// border:3px), so the lifted rects must equal scripts/m9_e2e/flexbox-ref.json.
//
//   cc -o examples/c/web-flexbox-simple.bin examples/c/web-flexbox-simple.c \
//      -lazul -Ltarget/release -Idll
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

// ── Direct-prop helpers (no string parsing) ─────────────────────────────────
static void add_prop(AzDom* d, AzCssProperty p) {
    AzDom_addCssProperty(d, AzCssPropertyWithConditions_simple(p));
}
static AzPixelValue PX(float v) { return AzPixelValue_px(v); }

static void set_border_box(AzDom* d) {
    add_prop(d, AzCssProperty_boxSizing(AzLayoutBoxSizing_borderBox()));
}
static void set_padding(AzDom* d, float v) {
    AzPixelValue p = PX(v);
    add_prop(d, AzCssProperty_paddingTop((AzLayoutPaddingTop){ .inner = p }));
    add_prop(d, AzCssProperty_paddingRight((AzLayoutPaddingRight){ .inner = p }));
    add_prop(d, AzCssProperty_paddingBottom((AzLayoutPaddingBottom){ .inner = p }));
    add_prop(d, AzCssProperty_paddingLeft((AzLayoutPaddingLeft){ .inner = p }));
}
// border:<w>px solid <c> on all 4 sides.
static void set_border(AzDom* d, float w, AzColorU c) {
    AzPixelValue p = PX(w);
    add_prop(d, AzCssProperty_borderTopWidth((AzLayoutBorderTopWidth){ .inner = p }));
    add_prop(d, AzCssProperty_borderRightWidth((AzLayoutBorderRightWidth){ .inner = p }));
    add_prop(d, AzCssProperty_borderBottomWidth((AzLayoutBorderBottomWidth){ .inner = p }));
    add_prop(d, AzCssProperty_borderLeftWidth((AzLayoutBorderLeftWidth){ .inner = p }));
    add_prop(d, AzCssProperty_borderTopStyle((AzStyleBorderTopStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(d, AzCssProperty_borderRightStyle((AzStyleBorderRightStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(d, AzCssProperty_borderBottomStyle((AzStyleBorderBottomStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(d, AzCssProperty_borderLeftStyle((AzStyleBorderLeftStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(d, AzCssProperty_borderTopColor((AzStyleBorderTopColor){ .inner = c }));
    add_prop(d, AzCssProperty_borderRightColor((AzStyleBorderRightColor){ .inner = c }));
    add_prop(d, AzCssProperty_borderBottomColor((AzStyleBorderBottomColor){ .inner = c }));
    add_prop(d, AzCssProperty_borderLeftColor((AzStyleBorderLeftColor){ .inner = c }));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;

    // body: box-sizing:border-box; padding:20px; width:800px; height:600px;
    AzDom body = AzDom_createBody();
    set_border_box(&body);
    set_padding(&body, 20.0f);
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(PX(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(PX(600.0f))));

    // container: box-sizing:border-box; display:flex; width:100%; height:100px; border:5px solid #000;
    AzDom container = AzDom_createDiv();
    set_border_box(&container);
    add_prop(&container, AzCssProperty_display(AzLayoutDisplay_flex()));
    add_prop(&container, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_percent(100.0f))));
    add_prop(&container, AzCssProperty_height(AzLayoutHeight_px(PX(100.0f))));
    set_border(&container, 5.0f, (AzColorU){ .r = 0, .g = 0, .b = 0, .a = 255 });

    // 3 flex items: box-sizing:border-box; flex-grow:1/2/3; border:3px solid #color;
    AzColorU item_colors[3] = {
        { .r = 0x88, .g = 0, .b = 0, .a = 255 },
        { .r = 0, .g = 0, .b = 0x88, .a = 255 },
        { .r = 0, .g = 0x88, .b = 0, .a = 255 },
    };
    for (int i = 0; i < 3; i++) {
        AzDom item = AzDom_createDiv();
        set_border_box(&item);
        add_prop(&item, AzCssProperty_flexGrow(AzLayoutFlexGrow_create((float)(i + 1))));
        set_border(&item, 3.0f, item_colors[i]);
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
