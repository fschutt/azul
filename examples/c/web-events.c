// S1 input-event coverage app for the azul WEB backend (2026-06-11).
//
// One div per event kind, each with exactly one callback (the web
// dispatcher is one-cb-per-node for now). Every callback bumps
// counters[EVT_KIND] in the shared model and returns DoNothing — the
// CDP test reads the counters straight out of wasm linear memory via
// AzStartup_peekU32(azModelPtr + 4*kind), so no DOM patching is needed
// to observe delivery.
//
// EVT kind slots (must match event_kind in dll/src/web/eventloop.rs):
//   0 click | 1 mousedown | 3 mousemove | 6 keydown | 10 resize
//   11 scroll | 12 mouseenter | 13 mouseleave | 14 contextmenu
#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

// ── Data model ──────────────────────────────────────────────────────────

typedef struct { uint32_t counters[16]; } EventCounters;
void EventCounters_destructor(void* m) { }

AzJson EventCounters_toJson(AzRefAny refany);
AzResultRefAnyString EventCounters_fromJson(AzJson json);
AZ_REFLECT_JSON(EventCounters, EventCounters_destructor, EventCounters_toJson, EventCounters_fromJson);

// The web pipeline only round-trips the model through JSON at hydrate
// time; the test starts from all-zero counters, so a constant works.
AzJson EventCounters_toJson(AzRefAny refany) {
    return AzJson_int(0);
}

AzResultRefAnyString EventCounters_fromJson(AzJson json) {
    EventCounters model;
    memset(&model, 0, sizeof(model));
    return AzResultRefAnyString_ok(EventCounters_upcast(model));
}

// ── Callbacks (one per event kind) ──────────────────────────────────────

static AzUpdate bump(AzRefAny* data, uint32_t slot) {
    EventCountersRefMut d = EventCountersRefMut_create(data);
    if (!EventCounters_downcastMut(data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counters[slot] += 1;
    EventCountersRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info)       { return bump(&data, 0); }
AzUpdate on_mousedown(AzRefAny data, AzCallbackInfo info)   { return bump(&data, 1); }
AzUpdate on_mousemove(AzRefAny data, AzCallbackInfo info)   { return bump(&data, 3); }
AzUpdate on_keydown(AzRefAny data, AzCallbackInfo info)     { return bump(&data, 6); }
AzUpdate on_resize(AzRefAny data, AzCallbackInfo info)      { return bump(&data, 10); }
AzUpdate on_scroll(AzRefAny data, AzCallbackInfo info)      { return bump(&data, 11); }
AzUpdate on_mouseenter(AzRefAny data, AzCallbackInfo info)  { return bump(&data, 12); }
AzUpdate on_mouseleave(AzRefAny data, AzCallbackInfo info)  { return bump(&data, 13); }
AzUpdate on_contextmenu(AzRefAny data, AzCallbackInfo info) { return bump(&data, 14); }

// ── Layout ──────────────────────────────────────────────────────────────

static AzDom event_div(const char* label, AzRefAny* data,
                       AzEventFilter filter, AzCallbackType cb) {
    AzDom text = AzDom_createText(AZ_STR(label));
    AzDom div = AzDom_createDiv();
    AzDom_addChild(&div, text);
    AzRefAny clone = AzRefAny_clone(data);
    AzDom_addCallback(&div, filter, clone, cb);
    return div;
}

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

    AzDom_addChild(&body, event_div("click me", &data,
        hover(AzHoverEventFilter_MouseUp), on_click));
    AzDom_addChild(&body, event_div("mousedown me", &data,
        hover(AzHoverEventFilter_MouseDown), on_mousedown));
    AzDom_addChild(&body, event_div("move over me", &data,
        hover(AzHoverEventFilter_MouseOver), on_mousemove));
    AzDom_addChild(&body, event_div("wheel over me", &data,
        hover(AzHoverEventFilter_Scroll), on_scroll));
    AzDom_addChild(&body, event_div("enter me", &data,
        hover(AzHoverEventFilter_MouseEnter), on_mouseenter));
    AzDom_addChild(&body, event_div("leave me", &data,
        hover(AzHoverEventFilter_MouseLeave), on_mouseleave));
    AzDom_addChild(&body, event_div("right-click me", &data,
        hover(AzHoverEventFilter_RightMouseUp), on_contextmenu));
    // Window-filter callbacks: fire regardless of pointer position
    // (broadcast path in AzStartup_dispatchEvent).
    AzDom_addChild(&body, event_div("resize watcher", &data,
        window_filter(AzWindowEventFilter_Resized), on_resize));
    AzDom_addChild(&body, event_div("keydown watcher", &data,
        window_filter(AzWindowEventFilter_VirtualKeyDown), on_keydown));

    return body;
}

// ── Main ────────────────────────────────────────────────────────────────

int main() {
    EventCounters model;
    memset(&model, 0, sizeof(model));
    AzRefAny data = EventCounters_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Web events");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
