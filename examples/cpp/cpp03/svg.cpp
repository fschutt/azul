// SVG Example - C++03
// g++ -std=c++03 -o svg svg.cpp -lazul

#include <azul.hpp>
using namespace azul;

struct AppData { int x; };
void AppData_destructor(AppData*) { }
AZ_REFLECT(AppData, AppData_destructor);

static const char* SVG_DATA = 
    "<svg viewBox='0 0 100 100'>"
    "  <circle cx='50' cy='50' r='40' fill='#3498db'/>"
    "  <rect x='30' y='30' width='40' height='40' fill='#e74c3c' opacity='0.7'/>"
    "</svg>";

static const char* CONTAINER_STYLE = 
    "width:100%; height:100%; display:flex; justify-content:center; align-items:center;";

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    Svg svg = Svg::from_string(SVG_DATA);
    SvgNode root = svg.get_root();
    
    Dom image = Dom::image(svg.render_to_image(400, 400));
    
    Dom container = Dom::div()
        .with_inline_style(CONTAINER_STYLE)
        .with_child(image);
    
    return container.style(Css::empty());
}

int main() {
    AppData model = { 0 };
    RefAny data = AppData::upcast(model);
    
    WindowCreateOptions window = WindowCreateOptions::new(layout);
    window.set_title("SVG Example");
    window.set_size(LogicalSize(500, 500));
    
    App app = App::new(data, AppConfig::default());
    app.run(window);
    
    return 0;
}
