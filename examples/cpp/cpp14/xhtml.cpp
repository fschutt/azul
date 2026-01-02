// g++ -std=c++14 -o xhtml xhtml.cpp -lazul

#include "azul14.hpp"
using namespace azul;

struct AppData { int x; };
AZ_REFLECT(AppData);

// Embedded XHTML content
static const char* XHTML_CONTENT = "<html><body><h1>Test XHTML</h1><p>This is a test spreadsheet.</p></body></html>";

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    return StyledDom::from_xml(String(XHTML_CONTENT)).release();
}

int main() {
    AppData model{0};
    RefAny data = AppData_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    
    return 0;
}
