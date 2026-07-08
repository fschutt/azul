// Memory test for the azul C++ binding. See tests/memtest/README.md.
//
// The harness measures RSS/gdb externally; this file just exercises the
// create/consume/DROP paths in a loop and exits 0. No event loop (App::run
// needs a display and hangs headless). The RAII wrappers (~AppConfig, ~Dom,
// ~App) call the Az*_delete functions on scope exit.
//
// Build (matches examples/cpp/cpp11/hello-world.cpp):
//   g++ -std=c++11 -o mem_test mem_test.cpp -lazul -L<target/release> \
//       -I<repo>/target/codegen

#include "azul11.hpp"
#include <cstdio>
#include <cstdlib>

using namespace azul;

struct MyDataModel { uint32_t counter; };

int main() {
    const char* n_env = std::getenv("AZ_MEMTEST_N");
    long n = n_env ? std::strtol(n_env, nullptr, 10) : 200000;
    if (n < 0) n = 0;

    // 1. The consume-by-value DROP path: App::create consumes RefAny +
    //    AppConfig; ~App destroys it. No run() -> no window.
    {
        MyDataModel model = { 5 };
        RefAny data = RefAny::create(std::move(model));
        App app = App::create(std::move(data), AppConfig::create());
    } // ~App here

    // 2. Leak loop: create/destroy droppable objects N times (RAII deletes).
    for (long i = 0; i < n; i++) {
        AppConfig cfg = AppConfig::create();
        Dom dom = Dom::create_body();
        (void)cfg;
        (void)dom;
    } // ~AppConfig + ~Dom each iteration

    std::printf("memtest cpp OK (N=%ld)\n", n);
    return 0;
}
