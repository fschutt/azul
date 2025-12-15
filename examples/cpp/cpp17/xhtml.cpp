// g++ -std=c++17 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
#include <fstream>
#include <sstream>
#include <string>

using namespace azul;
using namespace std::string_view_literals;

struct AppData { int x; };

auto read_file(std::string_view path) {
    std::ifstream file(std::string(path));
    std::stringstream buffer;
    buffer << file.rdbuf();
    return buffer.str();
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto xhtml = read_file("assets/spreadsheet.xhtml"sv);
    return StyledDom::from_xml(xhtml);
}

int main() {
    AppData model{.x = 0};
    auto data = RefAny::new(model);
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Spreadsheet"sv);
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
