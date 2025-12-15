// g++ -std=c++14 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
#include <fstream>
#include <sstream>
#include <string>
using namespace azul;

struct AppData { int x; };
AZ_REFLECT(AppData);

auto read_file(const std::string& path) {
    std::ifstream file(path);
    std::stringstream buffer;
    buffer << file.rdbuf();
    return buffer.str();
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto xhtml = read_file("assets/spreadsheet.xhtml");
    return StyledDom::from_xml(xhtml);
}

int main() {
    AppData model{0};
    auto data = RefAny::new(model);
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Spreadsheet");
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
