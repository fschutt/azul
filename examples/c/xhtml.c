// XHTML file loading and rendering example
// cc -o xhtml xhtml.c -lazul

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>

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

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    char* xhtml = read_file("assets/spreadsheet.xhtml");
    AzStyledDom dom = AzStyledDom_fromXml(AzString_copyFromBytes((uint8_t*)xhtml, 0, strlen(xhtml)));
    free(xhtml);
    return dom;
}

int main() {
    AzApp app = AzApp_new(AzRefAny_newC(NULL), AzAppConfig_default());
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("XHTML Spreadsheet");
    AzApp_run(&app, window);
    return 0;
}
