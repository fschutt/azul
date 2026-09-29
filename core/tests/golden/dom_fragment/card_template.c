#include "azul.h"
#include <string.h>
#include <stdarg.h>
#include <stdlib.h>

/* Joins NUL-terminated strings (the last argument is NULL) into an AzString. */
static AzString az_concat(const char* first, ...) {
    size_t len = 0;
    va_list ap;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) len += strlen(s);
    va_end(ap);
    char* buf = (char*)malloc(len + 1);
    size_t at = 0;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) {
        size_t n = strlen(s);
        memcpy(buf + at, s, n);
        at += n;
    }
    va_end(ap);
    AzString out = AzString_copyFromBytes((const uint8_t*)buf, 0, len);
    free(buf);
    return out;
}

AzDom render_card(const char* title, const char* text, const char* href, const char* author) {
    AzDom n0 = AzDom_createDiv();
    n0 = AzDom_withClass(n0, AZ_STR("card"));
    AzDom n1 = AzDom_createH2WithText(AZ_STR(title));
    AzDom_addChild(&n0, n1);
    AzDom n2 = AzDom_createPWithText(AZ_STR(text));
    AzDom_addChild(&n0, n2);
    AzDom n3 = AzDom_createANoA11y(AZ_STR(href), AzOptionString_some(AZ_STR("Read more")));
    AzDom_addChild(&n0, n3);
    AzDom n4 = AzDom_createSpanWithText(az_concat("by ", author, (const char*)NULL));
    AzDom_addChild(&n0, n4);
    return n0;
}
