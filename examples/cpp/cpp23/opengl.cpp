// OpenGL Integration - C++23
// g++ -std=c++23 -o opengl opengl.cpp -lazul

#include <azul.hpp>
#include <format>

using namespace azul;
using namespace std::string_view_literals;

struct OpenGlState {
    float rotation_deg = 0.0f;
    bool texture_uploaded = false;
};

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info);
Update on_startup(RefAny& data, CallbackInfo& info);
Update animate(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto body = Dom::body()
        .with_inline_style("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;"sv)
        .with_child(
            Dom::text("OpenGL Integration Demo"sv)
                .with_inline_style("color: white; font-size: 24px; margin-bottom: 20px;"sv)
        )
        .with_child(
            Dom::image(ImageRef::callback(data.clone(), render_texture))
                .with_inline_style(R"(
                    flex-grow: 1;
                    min-height: 300px;
                    border-radius: 10px;
                    box-shadow: 0px 0px 20px rgba(0,0,0,0.5);
                )"sv)
        );
    
    return body.style(Css::empty());
}

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info) {
    auto size = info.get_bounds().get_physical_size();
    
    auto d = OpenGlState::downcast_ref(data);
    if (!d) {
        return ImageRef::null_image(size.width, size.height, RawImageFormat::RGBA8, {});
    }
    
    auto gl_context = info.get_gl_context();
    if (!gl_context) {
        return ImageRef::null_image(size.width, size.height, RawImageFormat::RGBA8, {});
    }
    
    // Create and clear texture
    auto texture = Texture::allocate_rgba8(
        gl_context.value(),
        size,
        ColorU::from_str("#1a1a2e")
    );
    texture.clear();
    
    // Draw a rotating rectangle
    float rotation = d->rotation_deg;
    
    // Draw some shapes using the texture API
    texture.draw_rect(
        LogicalRect{100, 100, 200, 200},
        ColorU::from_str("#e94560"),
        {StyleTransform::Rotate(AngleValue::deg(rotation))}
    );
    
    texture.draw_rect(
        LogicalRect{150, 150, 100, 100},
        ColorU::from_str("#0f3460"),
        {StyleTransform::Rotate(AngleValue::deg(-rotation * 2))}
    );
    
    return ImageRef::gl_texture(texture);
}

Update on_startup(RefAny& data, CallbackInfo& info) {
    // Start animation timer
    info.start_timer(Timer::new(data.clone(), animate, info.get_system_time_fn()));
    return Update::DoNothing;
}

Update animate(RefAny& data, TimerCallbackInfo& info) {
    auto d = OpenGlState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->rotation_deg += 1.0f;
    if (d->rotation_deg >= 360.0f) {
        d->rotation_deg = 0.0f;
    }
    
    return Update::RefreshDom;
}

int main() {
    auto data = RefAny::new(OpenGlState{});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("OpenGL Integration"sv);
    window.set_size(LogicalSize(800, 600));
    window.set_on_create(data.clone(), on_startup);
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
