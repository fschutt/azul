// XHTML Example - C++14
// g++ -std=c++14 -o xhtml xhtml.cpp -lazul

#include <azul.hpp>
using namespace azul;

struct AppData { int x; };
AZ_REFLECT(AppData);

constexpr auto XHTML = R"(
<body style='display:flex; flex-direction:column; width:100%; height:100%; padding:40px;'>
  <h1 style='color:#ecf0f1; font-size:48px;'>XHTML Demo</h1>"
  <p style='color:#bdc3c7; font-size:18px;'>Loaded from XHTML string</p>
  <div style='display:flex; gap:20px;'>
    <div style='width:200px; height:150px; background:#3498db; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#e74c3c; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#2ecc71; border-radius:10px;'/>
  </div>
</body>
)";

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    return StyledDom::from_xml(XHTML);
}

int main() {
    AppData model{0};
    auto data = RefAny::new(model);
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("XHTML Example");
    window.set_size(LogicalSize(800, 400));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
