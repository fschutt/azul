#include "azul.h"
#include <stdio.h>
#include <string.h>
#include <time.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counter; int first_layout_done; } TimingData;
void TimingData_destructor(void* m) { }

AzJson TimingData_toJson(AzRefAny refany) { return AzJson_null(); }
AzResultRefAnyString TimingData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("not impl"));
}
AZ_REFLECT_JSON(TimingData, TimingData_destructor, TimingData_toJson, TimingData_fromJson);

static struct timespec g_t0;

static double ms_since_start() {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (now.tv_sec - g_t0.tv_sec) * 1000.0 +
           (now.tv_nsec - g_t0.tv_nsec) / 1000000.0;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    TimingDataRef ref = TimingDataRef_create(&data);
    if (TimingData_downcastRef(&data, &ref)) {
        if (!ref.ptr->first_layout_done) {
            printf("[%.1f ms] First layout callback (fonts resolved, DOM built)\n", ms_since_start());
        }
        TimingDataRef_delete(&ref);
    }

    AzDom body = AzDom_createBody();
    AzDom label = AzDom_createText(AZ_STR("Hello World - Timing Benchmark"));
    AzDom_addChild(&body, label);
    return AzDom_style(&body, AzCss_empty());
}

int main() {
    clock_gettime(CLOCK_MONOTONIC, &g_t0);
    printf("[%.1f ms] Program start\n", ms_since_start());

    TimingData model = { .counter = 0, .first_layout_done = 0 };
    AzRefAny data = TimingData_upcast(model);

    printf("[%.1f ms] Before App::create()\n", ms_since_start());
    AzApp app = AzApp_create(data, AzAppConfig_create());
    printf("[%.1f ms] After App::create()\n", ms_since_start());

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Timing Benchmark");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    printf("[%.1f ms] Before App::run() (window creation + first layout)\n", ms_since_start());
    AzApp_run(&app, window);
    printf("[%.1f ms] App::run() returned (window closed)\n", ms_since_start());
    AzApp_delete(&app);
    return 0;
}
