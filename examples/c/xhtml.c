// XHTML file loading and rendering example
// cc -o xhtml xhtml.c -lazul

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

char* read_file(const char* path) {
    FILE* f = fopen(path, "r");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = malloc(len + 1);
    fread(buf, 1, len, f);
    buf[len] = '\0';
    fclose(f);
    return buf;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    char* xhtml = read_file("assets/spreadsheet.xhtml");
    AzStyledDom dom = AzStyledDom_fromXml(AzString_copyFromBytes((uint8_t*)xhtml, 0, strlen(xhtml)));
    free(xhtml);
    return dom;
}

int main() {
    AzString empty_type = AzString_copyFromBytes((const uint8_t*)"", 0, 0);
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL);
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(empty_data, config);
    AzLayoutCallback layout_cb = { .cb = layout };
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_cb);
    AzString window_title = AzString_copyFromBytes((const uint8_t*)"XHTML Spreadsheet", 0, 17);
    window.window_state.title = window_title;
    AzApp_run(&app, window);
    return 0;
}
