// S1 minimal input-event app — few enough nodes to stay UNDER the class-B
// large-struct-sret trap that bites the 19-node web-events.c (hello-world's
// ~5 nodes lay out fine; this keeps a similar budget). Exercises the two S1
// routing paths that matter:
//   - single-target hit-test dispatch (click on a div)
//   - broadcast dispatch for Window-filter kinds (keydown, resize)
// CDP reads counters[] straight from wasm memory; no DOM patching needed.
#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counters[16]; } EventCounters;
void EventCounters_destructor(void* m) { }

AzJson EventCounters_toJson(AzRefAny refany);
AzResultRefAnyString EventCounters_fromJson(AzJson json);
AZ_REFLECT_JSON(EventCounters, EventCounters_destructor, EventCounters_toJson, EventCounters_fromJson);

AzJson EventCounters_toJson(AzRefAny refany) { return AzJson_int(0); }
AzResultRefAnyString EventCounters_fromJson(AzJson json) {
    EventCounters model;
    memset(&model, 0, sizeof(model));
    return AzResultRefAnyString_ok(EventCounters_upcast(model));
}

static AzUpdate bump(AzRefAny* data, uint32_t slot) {
    EventCountersRefMut d = EventCountersRefMut_create(data);
    if (!EventCounters_downcastMut(data, &d)) return AzUpdate_DoNothing;
    d.ptr->counters[slot] += 1;
    EventCountersRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info)   { return bump(&data, 0); }
AzUpdate on_keydown(AzRefAny data, AzCallbackInfo info)  { return bump(&data, 6); }
AzUpdate on_resize(AzRefAny data, AzCallbackInfo info)   { return bump(&data, 10); }

static AzEventFilter hover(AzHoverEventFilter h) {
    AzEventFilter f = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = h } };
    return f;
}
static AzEventFilter window_filter(AzWindowEventFilter w) {
    AzEventFilter f = { .Window = { .tag = AzEventFilter_Tag_Window, .payload = w } };
    return f;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom body = AzDom_createBody();

    // One callback PER node (the S1 dispatcher tracks a single kind per
    // node_idx). click = single-target hit-test; keydown/resize = Window-filter
    // broadcast (fire regardless of pointer position). 3 divs + 1 text ≈ 5
    // nodes — under the class-B trap threshold that bites the 19-node app.
    AzDom click_div = AzDom_createDiv();
    AzDom_addChild(&click_div, AzDom_createText(AZ_STR("click me")));
    AzRefAny c1 = AzRefAny_clone(&data);
    AzDom_addCallback(&click_div, hover(AzHoverEventFilter_MouseUp), c1, on_click);
    AzDom_addChild(&body, click_div);

    AzDom key_div = AzDom_createDiv();
    AzRefAny c2 = AzRefAny_clone(&data);
    AzDom_addCallback(&key_div, window_filter(AzWindowEventFilter_VirtualKeyDown), c2, on_keydown);
    AzDom_addChild(&body, key_div);

    AzDom resize_div = AzDom_createDiv();
    AzRefAny c3 = AzRefAny_clone(&data);
    AzDom_addCallback(&resize_div, window_filter(AzWindowEventFilter_Resized), c3, on_resize);
    AzDom_addChild(&body, resize_div);

    return body;
}

int main() {
    EventCounters model;
    memset(&model, 0, sizeof(model));
    AzRefAny data = EventCounters_upcast(model);
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Web events min");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
