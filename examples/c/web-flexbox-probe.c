// cb-OOB PINPOINT probe: full flexbox-simple DOM (which OOBs in the lifted cb during
// construction) instrumented with progress MARKers written to a free wasm addr
// (0x40570). The lifted cb runs ONLY in wasm (never natively — main() hands the cb
// to the web backend), so the absolute-address volatile store is safe and lands in
// shared linear memory the gate reads via peekU32(0x40570). The LAST marker before
// the OOB pinpoints which prop group (box-sizing / padding / border-width /
// border-style / border-color) triggers it. web-flexbox-min.c (width/height/display/
// flex-grow only) RUNS clean; this adds the by-value struct props back in groups.
//
//   cc -o examples/c/web-flexbox-probe.bin examples/c/web-flexbox-probe.c -lazul -Ltarget/release -Idll
#include "azul.h"

#define MARK(n) (*(volatile unsigned int*)(unsigned long)0x40570u = (unsigned int)(n))

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
static AzPixelValue PX(float v) { return AzPixelValue_px(v); }

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data; (void)info;
    MARK(1);

    AzDom body = AzDom_createBody();
    add_prop(&body, AzCssProperty_width(AzLayoutWidth_px(PX(800.0f))));
    add_prop(&body, AzCssProperty_height(AzLayoutHeight_px(PX(600.0f))));
    MARK(2);                                                    // width/height OK (min proven)
    add_prop(&body, AzCssProperty_boxSizing(AzLayoutBoxSizing_borderBox()));
    MARK(3);                                                    // + box-sizing
    {
        AzPixelValue p = PX(20.0f);
        add_prop(&body, AzCssProperty_paddingTop((AzLayoutPaddingTop){ .inner = p }));
        add_prop(&body, AzCssProperty_paddingRight((AzLayoutPaddingRight){ .inner = p }));
        add_prop(&body, AzCssProperty_paddingBottom((AzLayoutPaddingBottom){ .inner = p }));
        add_prop(&body, AzCssProperty_paddingLeft((AzLayoutPaddingLeft){ .inner = p }));
    }
    MARK(4);                                                    // + padding (×4)

    AzDom container = AzDom_createDiv();
    add_prop(&container, AzCssProperty_display(AzLayoutDisplay_flex()));
    add_prop(&container, AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_percent(100.0f))));
    add_prop(&container, AzCssProperty_height(AzLayoutHeight_px(PX(100.0f))));
    MARK(5);                                                    // + display/percent-width/height
    {
        AzPixelValue p = PX(5.0f);
        add_prop(&container, AzCssProperty_borderTopWidth((AzLayoutBorderTopWidth){ .inner = p }));
        add_prop(&container, AzCssProperty_borderRightWidth((AzLayoutBorderRightWidth){ .inner = p }));
        add_prop(&container, AzCssProperty_borderBottomWidth((AzLayoutBorderBottomWidth){ .inner = p }));
        add_prop(&container, AzCssProperty_borderLeftWidth((AzLayoutBorderLeftWidth){ .inner = p }));
    }
    MARK(6);                                                    // + border WIDTH (×4)
    add_prop(&container, AzCssProperty_borderTopStyle((AzStyleBorderTopStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(&container, AzCssProperty_borderRightStyle((AzStyleBorderRightStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(&container, AzCssProperty_borderBottomStyle((AzStyleBorderBottomStyle){ .inner = AzBorderStyle_Solid }));
    add_prop(&container, AzCssProperty_borderLeftStyle((AzStyleBorderLeftStyle){ .inner = AzBorderStyle_Solid }));
    MARK(7);                                                    // + border STYLE (×4)
    {
        AzColorU c = { .r = 0, .g = 0, .b = 0, .a = 255 };
        add_prop(&container, AzCssProperty_borderTopColor((AzStyleBorderTopColor){ .inner = c }));
        add_prop(&container, AzCssProperty_borderRightColor((AzStyleBorderRightColor){ .inner = c }));
        add_prop(&container, AzCssProperty_borderBottomColor((AzStyleBorderBottomColor){ .inner = c }));
        add_prop(&container, AzCssProperty_borderLeftColor((AzStyleBorderLeftColor){ .inner = c }));
    }
    MARK(8);                                                    // + border COLOR (×4)

    AzColorU item_colors[3] = {
        { .r = 0x88, .g = 0, .b = 0, .a = 255 },
        { .r = 0, .g = 0, .b = 0x88, .a = 255 },
        { .r = 0, .g = 0x88, .b = 0, .a = 255 },
    };
    for (int i = 0; i < 3; i++) {
        AzDom item = AzDom_createDiv();
        add_prop(&item, AzCssProperty_flexGrow(AzLayoutFlexGrow_create((float)(i + 1))));
        AzPixelValue p = PX(3.0f);
        add_prop(&item, AzCssProperty_borderTopWidth((AzLayoutBorderTopWidth){ .inner = p }));
        add_prop(&item, AzCssProperty_borderRightWidth((AzLayoutBorderRightWidth){ .inner = p }));
        add_prop(&item, AzCssProperty_borderBottomWidth((AzLayoutBorderBottomWidth){ .inner = p }));
        add_prop(&item, AzCssProperty_borderLeftWidth((AzLayoutBorderLeftWidth){ .inner = p }));
        add_prop(&item, AzCssProperty_borderTopColor((AzStyleBorderTopColor){ .inner = item_colors[i] }));
        AzDom_addChild(&container, item);
        MARK(10 + i);                                           // per-item done
    }
    MARK(50);

    AzDom_addChild(&body, container);
    MARK(99);                                                   // cb COMPLETE
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
