// cc -o xhtml xhtml.c -lazul

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Embedded XHTML content
static const char* XHTML_CONTENT = "<html><body><h1>Test XHTML</h1><p>This is a test spreadsheet.</p></body></html>";

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzString xml_str = AzString_copyFromBytes((const uint8_t*)XHTML_CONTENT, 0, strlen(XHTML_CONTENT));
    AzResultXmlXmlError result = AzXml_fromStr(xml_str);
    if (result.Ok.tag == 0) { // Ok variant
        return AzDom_createFromParsedXml(result.Ok.payload);
    }
    return AzDom_createBody();
}

int main() {
    AzString empty_type = AzString_copyFromBytes((const uint8_t*)"", 0, 0);
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL, 0, 0);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(empty_data, config);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString window_title = AzString_copyFromBytes((const uint8_t*)"XHTML Spreadsheet", 0, 17);
    window.window_state.title = window_title;
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
