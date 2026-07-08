// Memory test for the azul C binding. See tests/memtest/README.md / mem_test.py.
//
// The harness (scripts/run_memtest.sh) does the measuring:
//   - runs this under gdb           -> any SIGSEGV (double-free / UAF) fails
//   - runs it with a small and a large AZ_MEMTEST_N and compares peak RSS
//     -> RSS that scales with N is a LEAK
// So this file only exercises the create/consume/DROP paths in a loop and
// exits 0. No event loop (AzApp_run needs a display and hangs headless).
//
// Build (matches examples/c/run_all.sh):
//   cc -o mem_test mem_test.c -lazul -L<target/release> -I<repo>/dll
//   (azul.h lives in target/codegen; -I that dir too if needed)

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { (void)m; }
AZ_REFLECT(MyDataModel, MyDataModel_destructor)

int main(void) {
    const char* n_env = getenv("AZ_MEMTEST_N");
    long n = n_env ? strtol(n_env, NULL, 10) : 200000;
    if (n < 0) n = 0;

    // 1. The consume-by-value DROP path: AzApp_create consumes the AzRefAny +
    //    AzAppConfig (whose nested SystemStyle was one of the bitwise-cloned
    //    + double-freed types). Destroy the App without running it.
    {
        MyDataModel model = { .counter = 5 };
        AzRefAny data = MyDataModel_upcast(model);
        AzApp app = AzApp_create(data, AzAppConfig_create());
        AzApp_delete(&app);
    }

    // 2. Leak loop: create/destroy droppable objects N times.
    for (long i = 0; i < n; i++) {
        AzAppConfig cfg = AzAppConfig_create();
        AzAppConfig_delete(&cfg);

        AzDom dom = AzDom_createBody();
        AzDom_delete(&dom);
    }

    printf("memtest c OK (N=%ld)\n", n);
    return 0;
}
