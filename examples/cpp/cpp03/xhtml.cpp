// g++ -std=c++03 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
#include <fstream>
#include <sstream>
#include <string>
using namespace azul;

struct AppData { int x; };
void AppData_destructor(AppData*) { }
AZ_REFLECT(AppData, AppData_destructor);

std::string read_file(const char* path) {
    std::ifstream file(path);
    std::stringstream buffer;
    buffer << file.rdbuf();
    return buffer.str();
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    std::string xhtml = read_file("assets/spreadsheet.xhtml");
    return StyledDom::from_xml(xhtml);
}

int main() {
    AppData model = { 0 };
    RefAny data = AppData::upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Spreadsheet");
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
    
    return 0;
}
