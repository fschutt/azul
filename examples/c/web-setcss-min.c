// S2 minimal verification: a callback that modifies a CSS property via
// AzCallbackInfo_setCssProperty. Few enough nodes to stay under the class-B
// trap. on_click sets width:300px on the clicked node; the change rides a
// CallbackChange -> SET_INLINE_STYLE TLV -> JS el.style.setProperty. The CDP
// test asserts el.style.width flips to "300px".
#include "azul.h"
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t clicks; } Model;
void Model_destructor(void* m) { }

AzJson Model_toJson(AzRefAny refany);
AzResultRefAnyString Model_fromJson(AzJson json);
AZ_REFLECT_JSON(Model, Model_destructor, Model_toJson, Model_fromJson);

AzJson Model_toJson(AzRefAny refany) { return AzJson_int(0); }
AzResultRefAnyString Model_fromJson(AzJson json) {
    Model m; memset(&m, 0, sizeof(m));
    return AzResultRefAnyString_ok(Model_upcast(m));
}

// on_click: set width:300px on the hit node via the transaction API. Returns
// DoNothing — the visual change rides the CSS patch, no DOM rebuild needed.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    AzDomNodeId node = AzCallbackInfo_getHitNode(&info);
    AzCssProperty width = AzCssProperty_width(AzLayoutWidth_px(AzPixelValue_px(300.0)));
    AzCallbackInfo_setCssProperty(&info, node, width);
    return AzUpdate_DoNothing;
}

static AzEventFilter hover(AzHoverEventFilter h) {
    AzEventFilter f = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = h } };
    return f;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom body = AzDom_createBody();
    AzDom div = AzDom_createDiv();
    AzDom_addChild(&div, AzDom_createText(AZ_STR("click to widen")));
    AzRefAny clone = AzRefAny_clone(&data);
    AzDom_addCallback(&div, hover(AzHoverEventFilter_MouseUp), clone, on_click);
    AzDom_addChild(&body, div);
    return body;
}

int main() {
    Model m; memset(&m, 0, sizeof(m));
    AzRefAny data = Model_upcast(m);
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Web setcss min");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
