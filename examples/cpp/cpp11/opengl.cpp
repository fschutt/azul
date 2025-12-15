// g++ -std=c++11 -o opengl opengl.cpp -lazul

#include <azul.hpp>
#include <string>

using namespace azul;

struct OpenGlState {
    float rotation_deg;
    bool texture_uploaded;
    
    OpenGlState() : rotation_deg(0.0f), texture_uploaded(false) {}
};

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info);
Update on_startup(RefAny& data, CallbackInfo& info);
Update animate(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto body = Dom::body()
        .with_inline_style("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;")
        .with_child(
            Dom::text("OpenGL Integration Demo")
                .with_inline_style("color: white; font-size: 24px; margin-bottom: 20px;")
        )
        .with_child(
            Dom::image(ImageRef::callback(data.clone(), render_texture))
                .with_inline_style(
                    "flex-grow: 1;"
                    "min-height: 300px;"
                    "border-radius: 10px;"
                    "box-shadow: 0px 0px 20px rgba(0,0,0,0.5);"
                )
        );
    
    return body.style(Css::empty());
}

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info) {
    auto size = info.get_bounds().get_physical_size();
    
    OpenGlState* d = OpenGlState::downcast_ref(data);
    if (!d) {
        return ImageRef::null_image(size.width, size.height, RawImageFormat::RGBA8, std::vector<uint8_t>());
    }
    
    auto gl_context = info.get_gl_context();
    if (!gl_context) {
        return ImageRef::null_image(size.width, size.height, RawImageFormat::RGBA8, std::vector<uint8_t>());
    }
    
    auto texture = Texture::allocate_rgba8(
        gl_context.value(),
        size,
        ColorU::from_str("#1a1a2e")
    );
    texture.clear();
    
    float rotation = d->rotation_deg;
    
    std::vector<StyleTransform> transforms1;
    transforms1.push_back(StyleTransform::Rotate(AngleValue::deg(rotation)));
    texture.draw_rect(
        LogicalRect(100, 100, 200, 200),
        ColorU::from_str("#e94560"),
        transforms1
    );
    
    std::vector<StyleTransform> transforms2;
    transforms2.push_back(StyleTransform::Rotate(AngleValue::deg(-rotation * 2)));
    texture.draw_rect(
        LogicalRect(150, 150, 100, 100),
        ColorU::from_str("#0f3460"),
        transforms2
    );
    
    return ImageRef::gl_texture(texture);
}

Update on_startup(RefAny& data, CallbackInfo& info) {
    info.start_timer(Timer::new_timer(data.clone(), animate, info.get_system_time_fn()));
    return Update::DoNothing;
}

Update animate(RefAny& data, TimerCallbackInfo& info) {
    OpenGlState* d = OpenGlState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->rotation_deg += 1.0f;
    if (d->rotation_deg >= 360.0f) {
        d->rotation_deg = 0.0f;
    }
    
    return Update::RefreshDom;
}

int main() {
    OpenGlState state;
    auto data = RefAny::new_ref(state);
    
    auto window = WindowCreateOptions::new_window(layout);
    window.set_title("OpenGL Integration");
    window.set_size(LogicalSize(800, 600));
    window.set_on_create(data.clone(), on_startup);
    
    auto app = App::new_app(data, AppConfig::default_config());
    app.run(window);
    return 0;
}
