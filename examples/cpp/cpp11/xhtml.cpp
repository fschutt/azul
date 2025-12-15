// g++ -std=c++11 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
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

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    std::string xhtml = read_file("assets/spreadsheet.xhtml");
    return StyledDom::from_xml(xhtml);
}

int main() {
    AppData model{0};
    RefAny data = RefAny::new(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Spreadsheet");
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
}
