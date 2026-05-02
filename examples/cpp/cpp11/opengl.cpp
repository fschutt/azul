// g++ -std=c++11 -o opengl opengl.cpp -lazul
// Note: This example is simplified as OpenGL texture integration requires more complex setup

#include "azul11.hpp"
#include <string>

using namespace azul;

struct OpenGlState {
    float rotation_deg;
    bool texture_uploaded;

    OpenGlState() : rotation_deg(0.0f), texture_uploaded(false) {}
};

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const OpenGlState* d = data_wrapper.downcast_ref<OpenGlState>();
    if (!d) return AzDom_createBody();

    Dom title = Dom::create_text(String("OpenGL Integration Demo"));
    title.set_css(String("color: white; font-size: 24px; margin-bottom: 20px;"));

    Dom placeholder = Dom::create_text(String("OpenGL texture would render here"));
    placeholder.set_css(String("flex-grow: 1; min-height: 300px; border-radius: 10px; background: #333; color: white; display: flex; align-items: center; justify-content: center;"));

    Dom body = Dom::create_body();
    body.set_css(String("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;"));
    body.add_child(std::move(title));
    body.add_child(std::move(placeholder));

    return body.style(Css::empty()).release();
}

int main() {
    OpenGlState state;
    RefAny data = RefAny::create(std::move(state));

    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
