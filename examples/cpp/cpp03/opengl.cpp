#include "azul03.hpp"

using namespace azul;

struct OpenGlState {
    float rotation_deg;
};
AZ_REFLECT(OpenGlState);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const OpenGlState* d = OpenGlState_downcast_ref(data_wrapper);
    if (!d) return AzDom_createBody();
    
    Dom title = Dom::create_p_with_text(String("OpenGL Integration Demo"));
    title.set_css(String("color: white; font-size: 24px; margin-bottom: 20px;"));
    
    Dom placeholder = Dom::create_p_with_text(String("OpenGL texture would render here"));
    placeholder.set_css(String("flex-grow: 1; min-height: 300px; border-radius: 10px; background: #333; color: white; display: flex; align-items: center; justify-content: center;"));
    
    Dom body = Dom::create_body();
    body.set_css(String("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;"));
    body.add_child(title);
    body.add_child(placeholder);
    
    return body.release();
}

int main() {
    OpenGlState state;
    state.rotation_deg = 0.0f;
    RefAny data = OpenGlState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = az_string_from_literal("OpenGL Integration");
    window.inner().window_state.size.dimensions.width = 800.0;
    window.inner().window_state.size.dimensions.height = 600.0;
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
