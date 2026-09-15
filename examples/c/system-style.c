#include "azul.h"
#include <stdio.h>
#include <string.h>

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
