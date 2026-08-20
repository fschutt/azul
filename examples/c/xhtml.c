#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define DOC_PATH "../assets/spreadsheet.xhtml"

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzDom error_dom(AzString detail) {
    AzDom heading = AzDom_createDivWithText(str("XHTML load failed"));
    AzDom_setCss(&heading, str("font-size: 20px; font-weight: bold; color: #a61b1b; margin-bottom: 8px;"));

    AzDom message = AzDom_createDivWithText(detail);
    AzDom_setCss(&message, str("font-size: 13px; color: #5f2120;"));

    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, str("display: flex; flex-direction: column; padding: 24px; background: #fdf2f2;"));
    AzDom_addChild(&body, heading);
    AzDom_addChild(&body, message);
    return body;
}

static char* read_file(const char* path, size_t* out_len) {
    FILE* f = fopen(path, "rb");
    if (!f) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    char* buf = (char*)malloc((size_t)size + 1);
    if (!buf) {
        fclose(f);
        return NULL;
    }
    size_t read = fread(buf, 1, (size_t)size, f);
    fclose(f);
    buf[read] = '\0';
    *out_len = read;
    return buf;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    size_t len = 0;
    char* source = read_file(DOC_PATH, &len);
    if (!source) {
        return error_dom(str("cannot open " DOC_PATH));
    }

    AzString xml_str = AzString_copyFromBytes((const uint8_t*)source, 0, len);
    free(source);

    AzResultXmlXmlError result = AzXml_fromStr(xml_str);
    if (result.Ok.tag != AzResultXmlXmlError_Tag_Ok) {
        return error_dom(AzXmlError_toDbgString(&result.Err.payload));
    }
    return AzDom_createFromParsedXml(result.Ok.payload);
}

int main() {
    AzString empty_type = str("");
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL, 0, 0);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(empty_data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = str("Book1 - Excel");
    window.window_state.size.dimensions.width = 1100.0;
    window.window_state.size.dimensions.height = 720.0;
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
