#include "azul.h"
#include <stdio.h>
#include <string.h>
#include <time.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counter; } BenchData;
void BenchData_destructor(void* m) { }

AzJson BenchData_toJson(AzRefAny refany) { return AzJson_null(); }
AzResultRefAnyString BenchData_fromJson(AzJson json) {
    return AzResultRefAnyString_err(AZ_STR("not impl"));
}
AZ_REFLECT_JSON(BenchData, BenchData_destructor, BenchData_toJson, BenchData_fromJson);

static double ms_since(struct timespec *start) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (now.tv_sec - start->tv_sec) * 1000.0 +
           (now.tv_nsec - start->tv_nsec) / 1000000.0;
}

int main() {
    struct timespec t0;
    clock_gettime(CLOCK_MONOTONIC, &t0);

    BenchData model = { .counter = 0 };
    AzRefAny data = BenchData_upcast(model);
    AzAppConfig config = AzAppConfig_create();

    printf("[%.1f ms] Before App::create()\n", ms_since(&t0));
    AzApp app = AzApp_create(data, config);
    printf("[%.1f ms] After App::create()\n", ms_since(&t0));

    printf("[%.1f ms] Benchmark complete — exiting without opening window\n", ms_since(&t0));

    // Don't run the event loop — just measure create() time
    AzApp_delete(&app);
    return 0;
}
