// g++ -std=c++03 -o opengl opengl.cpp -lazul

#include <azul.hpp>
#include <string>
#include <vector>

using namespace azul;

struct OpenGlState {
    float rotation_deg;
    bool texture_uploaded;
};

void OpenGlState_init(OpenGlState* s) {
    s->rotation_deg = 0.0f;
    s->texture_uploaded = false;
}

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info);
Update on_startup(RefAny& data, CallbackInfo& info);
Update animate(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    Dom title = Dom_text("OpenGL Integration Demo");
    Dom_setInlineStyle(title, "color: white; font-size: 24px; margin-bottom: 20px;");
    
    Dom image = Dom_image(ImageRef_callback(RefAny_clone(data), render_texture));
    Dom_setInlineStyle(image, 
        "flex-grow: 1;"
        "min-height: 300px;"
        "border-radius: 10px;"
        "box-shadow: 0px 0px 20px rgba(0,0,0,0.5);");
    
    Dom body = Dom_body();
    Dom_setInlineStyle(body, "
        background: linear-gradient(#1a1a2e, #16213e); 
        padding: 20px;
    ");
    Dom_addChild(body, title);
    Dom_addChild(body, image);
    
    return StyledDom_new(body, Css_empty());
}

ImageRef render_texture(RefAny& data, RenderImageCallbackInfo& info) {
    PhysicalSizeU32 size = RenderImageCallbackInfo_getBounds(info).get_physical_size();
    
    OpenGlState* d = OpenGlState_downcast_ref(data);
    if (!d) {
        std::vector<uint8_t> empty;
        return ImageRef_nullImage(size.width, size.height, RawImageFormat_RGBA8, empty);
    }
    
    GlContextPtr gl_context = RenderImageCallbackInfo_getGlContext(info);
    if (!gl_context) {
        std::vector<uint8_t> empty;
        return ImageRef_nullImage(size.width, size.height, RawImageFormat_RGBA8, empty);
    }
    
    Texture texture = Texture_allocateRgba8(gl_context, size, ColorU_fromStr("#1a1a2e"));
    Texture_clear(texture);
    
    float rotation = d->rotation_deg;
    
    LogicalRect rect1;
    rect1.x = 100; rect1.y = 100; rect1.width = 200; rect1.height = 200;
    std::vector<StyleTransform> transforms1;
    transforms1.push_back(StyleTransform_Rotate(AngleValue_deg(rotation)));
    Texture_drawRect(texture, rect1, ColorU_fromStr("#e94560"), transforms1);
    
    LogicalRect rect2;
    rect2.x = 150; rect2.y = 150; rect2.width = 100; rect2.height = 100;
    std::vector<StyleTransform> transforms2;
    transforms2.push_back(StyleTransform_Rotate(AngleValue_deg(-rotation * 2)));
    Texture_drawRect(texture, rect2, ColorU_fromStr("#0f3460"), transforms2);
    
    return ImageRef_glTexture(texture);
}

Update on_startup(RefAny& data, CallbackInfo& info) {
    Timer timer = Timer_new(RefAny_clone(data), animate, CallbackInfo_getSystemTimeFn(info));
    CallbackInfo_startTimer(info, timer);
    return Update_DoNothing;
}

Update animate(RefAny& data, TimerCallbackInfo& info) {
    OpenGlState* d = OpenGlState_downcast_mut(data);
    if (!d) return Update_DoNothing;
    
    d->rotation_deg += 1.0f;
    if (d->rotation_deg >= 360.0f) {
        d->rotation_deg = 0.0f;
    }
    
    return Update_RefreshDom;
}

int main() {
    OpenGlState state;
    OpenGlState_init(&state);
    RefAny data = OpenGlState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions_new(layout);
    WindowCreateOptions_setTitle(window, "OpenGL Integration");
    WindowCreateOptions_setSize(window, LogicalSize_new(800, 600));
    WindowCreateOptions_setOnCreate(window, RefAny_clone(data), on_startup);
    
    App app = App_new(data, AppConfig_default());
    App_run(app, window);
    return 0;
}
