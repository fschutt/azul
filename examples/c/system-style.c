/**
 * system-style.c — Detect and print all SystemStyle values.
 *
 * Build:
 *   cc -o target/c-examples/system-style examples/c/system-style.c \
 *      -Itarget/codegen \
 *      -framework Cocoa -framework OpenGL -framework IOKit \
 *      -framework CoreFoundation -framework CoreGraphics \
 *      -Ltarget/release -lazul -Wl,-rpath,target/release
 *
 * Run:
 *   ./target/c-examples/system-style
 */
#include "azul.h"
#include <stdio.h>
#include <string.h>

/* Helper: print an AzString to stdout and delete it. */
static void print_az_string(AzString* s) {
    if (s->vec.ptr && s->vec.len > 0) {
        fwrite(s->vec.ptr, 1, s->vec.len, stdout);
    }
    AzString_delete(s);
}

int main(void) {
    AzSystemStyle style = AzSystemStyle_detect();
    AzString dbg = AzSystemStyle_toDbgString(&style);
    print_az_string(&dbg);
    printf("\n");
    AzSystemStyle_delete(&style);
    return 0;
}
