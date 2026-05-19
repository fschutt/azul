// M11 Sprint 6 — TodoMVC-style flat bench.
//
// Renders a list of N rows + one "run" button. Each iteration of
// the bench harness:
//   1. Click "run" → cb generates fresh row data + returns
//      RefreshDom.
//   2. Wasm-side re-layout + diff against the previous tree.
//   3. JS-side patches applied to DOM.
//   4. Harness measures wall-clock from click → DOM stable.
//
// This is the "flat" variant — N rows rendered directly. The user
// directive (M11 hard rule #2) wants us to publish numbers
// alongside React/Preact/Svelte: this variant matches their
// render-all approach at moderate N=1000. The 10k variant
// (`azul-bench-virtual.c`) is staged separately + uses VirtualView
// to render only the visible slice.

#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

// ── Data model ─────────────────────────────────────────────────

#define BENCH_FLAT_N 1000

typedef struct {
    uint32_t next_id;
    uint32_t selected_id;
    uint32_t row_count;
} BenchModel;

void BenchModel_destructor(void* m) { }
AzJson BenchModel_toJson(AzRefAny refany);
AzResultRefAnyString BenchModel_fromJson(AzJson json);
AZ_REFLECT_JSON(BenchModel, BenchModel_destructor, BenchModel_toJson, BenchModel_fromJson);

AzJson BenchModel_toJson(AzRefAny refany) { return AzJson_int(0); }
AzResultRefAnyString BenchModel_fromJson(AzJson json) {
    BenchModel m = { .next_id = 1, .selected_id = 0, .row_count = 0 };
    return AzResultRefAnyString_ok(BenchModel_upcast(m));
}

// ── Callbacks ──────────────────────────────────────────────────

AzUpdate on_run(AzRefAny data, AzCallbackInfo info) {
    BenchModelRefMut d = BenchModelRefMut_create(&data);
    if (BenchModel_downcastMut(&data, &d)) {
        d.ptr->row_count = BENCH_FLAT_N;
        d.ptr->next_id += BENCH_FLAT_N;
        BenchModelRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    return AzUpdate_DoNothing;
}

AzUpdate on_clear(AzRefAny data, AzCallbackInfo info) {
    BenchModelRefMut d = BenchModelRefMut_create(&data);
    if (BenchModel_downcastMut(&data, &d)) {
        d.ptr->row_count = 0;
        BenchModelRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    return AzUpdate_DoNothing;
}

AzUpdate on_swap(AzRefAny data, AzCallbackInfo info) {
    BenchModelRefMut d = BenchModelRefMut_create(&data);
    if (BenchModel_downcastMut(&data, &d)) {
        // Bump next_id as a sentinel that the row labels changed.
        d.ptr->next_id += 2;
        BenchModelRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    return AzUpdate_DoNothing;
}

// ── Layout ─────────────────────────────────────────────────────

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    BenchModelRef d = BenchModelRef_create(&data);
    if (!BenchModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    AzDom body = AzDom_createBody();

    // Header row: "run" + "clear" + "swap" buttons.
    AzButton btn_run = AzButton_create(AZ_STR("run"));
    AzRefAny ref_run = AzRefAny_clone(&data);
    AzButton_setOnClick(&btn_run, ref_run, on_run);
    AzDom_addChild(&body, AzButton_dom(btn_run));

    AzButton btn_clear = AzButton_create(AZ_STR("clear"));
    AzRefAny ref_clear = AzRefAny_clone(&data);
    AzButton_setOnClick(&btn_clear, ref_clear, on_clear);
    AzDom_addChild(&body, AzButton_dom(btn_clear));

    AzButton btn_swap = AzButton_create(AZ_STR("swap"));
    AzRefAny ref_swap = AzRefAny_clone(&data);
    AzButton_setOnClick(&btn_swap, ref_swap, on_swap);
    AzDom_addChild(&body, AzButton_dom(btn_swap));

    // N rows. Each row gets a unique label. The diff loop in
    // Sprint 3 will produce SetText patches per row when labels
    // change (e.g. after `swap`).
    uint32_t row_count = d.ptr->row_count;
    uint32_t base_id = d.ptr->next_id - row_count;
    char buf[32];
    for (uint32_t i = 0; i < row_count; i++) {
        int len = snprintf(buf, sizeof(buf), "row #%u (id=%u)", i, base_id + i);
        AzString label = AzString_copyFromBytes((const uint8_t*)buf, 0, len);
        AzDom row = AzDom_createDiv();
        AzDom_addChild(&row, AzDom_createText(label));
        AzDom_addChild(&body, row);
    }

    BenchModelRef_delete(&d);
    return body;
}

// ── Main ───────────────────────────────────────────────────────

int main() {
    BenchModel m = { .next_id = 1, .selected_id = 0, .row_count = 0 };
    AzRefAny data = BenchModel_upcast(m);
    AzWindowCreateOptions w = AzWindowCreateOptions_create(layout);
    w.window_state.title = AZ_STR("Azul Flat Bench");
    AzApp a = AzApp_create(data, AzAppConfig_default());
    AzApp_run(&a, w);
    return 0;
}
