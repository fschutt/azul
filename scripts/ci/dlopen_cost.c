/* What dlopen() of each named library costs in this process, in the order named: the libraries a library pulls in
 * are loaded by the first one that needs them, so the later ones show only their own share.
 *   cc -O1 -o dlopen_cost scripts/ci/dlopen_cost.c -ldl && ./dlopen_cost libX11.so.6 libEGL.so.1 libgtk-3.so.0
 * Each name is opened with RTLD_NOW (the engine's loader does the same), then its first symbol lookup is timed too:
 * with one more argument "gtk_im_context_simple_new" after "--" the symbol is called once (libgtk-3's IM context). */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1.0e6;
}

int main(int argc, char **argv) {
    int i = 1;
    double total = 0.0;
    for (; i < argc && strcmp(argv[i], "--") != 0; i++) {
        double t0 = now_ms();
        void *h = dlopen(argv[i], RTLD_NOW | RTLD_GLOBAL);
        double dt = now_ms() - t0;
        total += dt;
        printf("%-34s %8.2f ms %s\n", argv[i], dt, h ? "" : "(not loaded)");
    }
    if (i + 1 < argc) {
        const char *symbol = argv[i + 1];
        void *(*fn)(void) = (void *(*)(void))dlsym(RTLD_DEFAULT, symbol);
        if (fn) {
            double t0 = now_ms();
            void *r = fn();
            double dt = now_ms() - t0;
            total += dt;
            printf("%-34s %8.2f ms (call, result %s)\n", symbol, dt, r ? "non-null" : "null");
        } else {
            printf("%-34s not found\n", symbol);
        }
    }
    printf("%-34s %8.2f ms\n", "total", total);
    return 0;
}
