// g++ -std=c++11 -o xhtml xhtml.cpp -lazul

#include "azul23.hpp"
#include <fstream>
#include <sstream>
#include <string>
using namespace azul;

struct AppData { int x; };
AZ_REFLECT(AppData);

std::string read_file(const std::string& path) {
    std::ifstream file(path);
    std::stringstream buffer;
    buffer << file.rdbuf();
    return buffer.str();
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    std::string xhtml = read_file("assets/spreadsheet.xhtml");
    return StyledDom::from_xml(String(xhtml.c_str())).release();
}

int main() {
    AppData model{0};
    RefAny data = AppData_upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    
    return 0;
}
