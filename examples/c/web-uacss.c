// ua_css (S7) Chrome-parity verification vehicle for the web backend.
// Renders representative tags through the real XML→DOM path so each
// element gets its NodeType-based UA style from core/src/ua_css.rs.
// cc -o web-uacss.bin web-uacss.c -I. -L../../target/aarch64-apple-darwin/release -lazul
#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t unused; } UaModel;
void UaModel_destructor(void* m) { }

AzJson UaModel_toJson(AzRefAny refany);
AzResultRefAnyString UaModel_fromJson(AzJson json);
AZ_REFLECT_JSON(UaModel, UaModel_destructor, UaModel_toJson, UaModel_fromJson);

AzJson UaModel_toJson(AzRefAny refany) { return AzJson_int(0); }
AzResultRefAnyString UaModel_fromJson(AzJson json) {
    UaModel model = { .unused = 0 };
    return AzResultRefAnyString_ok(UaModel_upcast(model));
}

static const char* XHTML_CONTENT =
    "<html><body>"
    "<h1>Heading one</h1>"
    "<h2>Heading two</h2>"
    "<p>First paragraph text.</p>"
    "<p>Second paragraph text.</p>"
    "<div>Plain div text.</div>"
    "</body></html>";

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzString xml_str = AzString_copyFromBytes(
        (const uint8_t*)XHTML_CONTENT, 0, strlen(XHTML_CONTENT));
    AzResultXmlXmlError result = AzXml_fromStr(xml_str);
    if (result.Ok.tag == 0) {
        return AzDom_createFromParsedXml(result.Ok.payload);
    }
    return AzDom_createBody();
}

int main() {
    UaModel model = { .unused = 0 };
    AzRefAny data = UaModel_upcast(model);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("uacss");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
