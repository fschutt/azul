// g++ -std=c++11 -o opengl opengl.cpp -lazul
// Note: This example is simplified as OpenGL texture integration requires more complex setup

#include "azul14.hpp"
#include <string>

using namespace azul;

struct OpenGlState {
    float rotation_deg;
    bool texture_uploaded;
    
    OpenGlState() : rotation_deg(0.0f), texture_uploaded(false) {}
};
AZ_REFLECT(OpenGlState);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const OpenGlState* d = OpenGlState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    Dom title = Dom::create_text("OpenGL Integration Demo");
    title.set_inline_style("color: white; font-size: 24px; margin-bottom: 20px;");
    
    Dom placeholder = Dom::create_text("OpenGL texture would render here");
    placeholder.set_inline_style("flex-grow: 1; min-height: 300px; border-radius: 10px; background: #333; color: white; display: flex; align-items: center; justify-content: center;");
    
    Dom body = Dom::create_body();
    body.set_inline_style("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;");
    body.add_child(std::move(title));
    body.add_child(std::move(placeholder));
    
    return body.style(Css::empty()).release();
}

int main() {
    OpenGlState state;
    RefAny data = OpenGlState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"OpenGL Integration", 0, 18);
    window.inner().window_state.size.dimensions.width = 800.0;
    window.inner().window_state.size.dimensions.height = 600.0;
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
