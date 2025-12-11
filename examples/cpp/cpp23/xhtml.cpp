// XHTML Example - C++23
// g++ -std=c++23 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
using namespace azul;
using namespace std::string_view_literals;

// No AZ_REFLECT needed in C++17+
struct AppData { int x; };

inline constexpr auto XHTML = R"(
<body style='display:flex; flex-direction:column; width:100%; height:100%; padding:40px;'>
  <h1 style='color:#ecf0f1; font-size:48px;'>XHTML Demo</h1>"
  <p style='color:#bdc3c7; font-size:18px;'>Loaded from XHTML string</p>
  <div style='display:flex; gap:20px;'>
    <div style='width:200px; height:150px; background:#3498db; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#e74c3c; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#2ecc71; border-radius:10px;'/>
  </div>
</body>
)"sv;

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    return StyledDom::from_xml(XHTML);
}

int main() {
    auto data = RefAny::new(AppData{.x = 0});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Example"sv);
    window.set_size(LogicalSize(800, 400));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
