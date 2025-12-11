// SVG Example - C
// cc -o svg svg.c -lazul

#include <azul.h>

typedef struct { int x; } AppData;
void AppData_destructor(AppData* p) { }
AZ_REFLECT(AppData, AppData_destructor);

static const AzString SVG_DATA = AzString_fromConstStr(
    "<svg viewBox='0 0 100 100'>"
    "  <circle cx='50' cy='50' r='40' fill='#3498db'/>"
    "  <rect x='30' y='30' width='40' height='40' fill='#e74c3c' opacity='0.7'/>"
    "</svg>"
);

static const AzString CONTAINER_STYLE = AzString_fromConstStr(
    "width:100%; height:100%; display:flex; justify-content:center; align-items:center;"
);

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    AzSvg svg = AzSvg_fromString(SVG_DATA);
    AzDom image = AzDom_image(AzSvg_renderToImage(&svg, 400, 400));
    
    AzDom container = AzDom_div();
    AzDom_setInlineStyle(&container, CONTAINER_STYLE);
    AzDom_addChild(&container, image);
    
    return AzDom_style(&container, AzCss_empty());
}

int main() {
    AppData model = { 0 };
    AzRefAny data = AppData_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("SVG Example");
    window.state.size.dimensions.width = 500.0;
    window.state.size.dimensions.height = 500.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
