// g++ -std=c++14 -o xhtml xhtml.cpp -lazul

#include "azul14.hpp"
using namespace azul;

struct AppData { int x; };

// Embedded XHTML content
static const char* XHTML_CONTENT = "<html><body><h1>Test XHTML</h1><p>This is a test spreadsheet.</p></body></html>";

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    String src(XHTML_CONTENT);
    AzResultXmlXmlError parsed = AzXml_fromStr(src.release());
    if (parsed.Ok.tag != AzResultXmlXmlError_Tag_Ok) {
        AzResultXmlXmlError_delete(&parsed);
        return AzDom_createBody();
    }
    // Move the parsed AzXml out of the Result, then build a Dom from it.
    AzXml xml = parsed.Ok.payload;
    parsed = {};
    return AzDom_createFromParsedXml(xml);
}

int main() {
    AppData model{0};
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));

    return 0;
}
