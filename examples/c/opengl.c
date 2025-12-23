// OpenGL Integration - C
// cc -o opengl opengl.c -lazul

#include <azul.h>
#include <stdio.h>
#include <math.h>
#include <time.h>

// Helper function to create a system time callback
AzInstant get_current_system_time(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    AzInstantPtr ptr;
    ptr.ptr = NULL;
    ptr.clone_fn.cb = NULL;
    ptr.destructor.cb = NULL;
    AzSystemTick tick = { .tick_counter = (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec };
    return AzInstant_tick(tick);
}

typedef struct {
    float rotation_deg;
} OpenGlState;

void OpenGlState_destructor(void* s) { }
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info);
AzUpdate on_startup(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzString title_text = AzString_copyFromBytes((const uint8_t*)"OpenGL Integration Demo", 0, 23);
    AzDom title = AzDom_createText(title_text);
    AzString title_style = AzString_copyFromBytes((const uint8_t*)"color: white; font-size: 24px; margin-bottom: 20px;", 0, 52);
    AzDom_setInlineStyle(&title, title_style);
    
    // Create a callback-based image
    AzCoreRenderImageCallback callback = { 
        .cb = (AzCoreRenderImageCallbackType)render_texture, 
        .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } }
    };
    AzDom image = AzDom_createImage(AzImageRef_callback(callback, AzRefAny_clone(&data)));
    AzString image_style = AzString_copyFromBytes((const uint8_t*)
        "flex-grow: 1;"
        "min-height: 300px;"
        "border-radius: 10px;"
        "box-shadow: 0px 0px 20px rgba(0,0,0,0.5);", 0, 94);
    AzDom_setInlineStyle(&image, image_style);
    
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"background: linear-gradient(#1a1a2e, #16213e); padding: 20px;", 0, 62);
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, image);
    
    return AzDom_style(&body, AzCss_empty());
}

AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info) {
    AzHidpiAdjustedBounds bounds = AzRenderImageCallbackInfo_getBounds(&info);
    AzPhysicalSizeU32 size = AzHidpiAdjustedBounds_getPhysicalSize(&bounds);
    
    OpenGlStateRef d = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d)) {
        // Return invalid image ref - just return empty texture
        AzCoreRenderImageCallback empty_cb = { .cb = 0, .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } } };
        return AzImageRef_callback(empty_cb, data);
    }
    
    AzOptionGlContextPtr opt_gl = AzRenderImageCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        OpenGlStateRef_delete(&d);
        AzCoreRenderImageCallback empty_cb = { .cb = 0, .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } } };
        return AzImageRef_callback(empty_cb, data);
    }
    
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    float rotation = d.ptr->rotation_deg;
    OpenGlStateRef_delete(&d);
    
    // Create a solid colored texture
    AzString color_str = AzString_copyFromBytes((const uint8_t*)"#1a1a2e", 0, 7);
    AzColorU bg_color = AzColorU_fromStr(color_str);
    AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
    AzTexture_clear(&texture);
    
    // For now, just return the cleared texture as a demo
    // More advanced drawing would require GL calls which aren't exposed in C API
    return AzImageRef_glTexture(texture);
}

AzUpdate on_startup(AzRefAny data, AzCallbackInfo info) {
    AzSystemTimeDiff interval_diff = AzSystemTimeDiff_fromMillis(16);
    AzDuration interval = { .System = { .tag = AzDuration_Tag_System, .payload = interval_diff } };
    AzGetSystemTimeCallback time_fn = { .cb = get_current_system_time };
    AzTimer timer = AzTimer_create(AzRefAny_clone(&data), (AzTimerCallbackType)animate, time_fn);
    timer = AzTimer_withInterval(timer, interval);
    AzTimerId timer_id = AzTimerId_unique();
    AzCallbackInfo_addTimer(&info, timer_id, timer);
    return AzUpdate_DoNothing;
}

AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info) {
    OpenGlStateRefMut d = OpenGlStateRefMut_create(&data);
    if (!OpenGlState_downcastMut(&data, &d)) {
        return AzTimerCallbackReturn_terminateUnchanged();
    }
    
    d.ptr->rotation_deg += 1.0f;
    if (d.ptr->rotation_deg >= 360.0f) {
        d.ptr->rotation_deg = 0.0f;
    }
    OpenGlStateRefMut_delete(&d);
    
    return AzTimerCallbackReturn_continueAndUpdate();
}

int main() {
    OpenGlState state = { .rotation_deg = 0.0f };
    AzRefAny data = OpenGlState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString title = AzString_copyFromBytes((const uint8_t*)"OpenGL Integration", 0, 18);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    
    // Set onCreate callback
    AzCallback on_create_cb = { .cb = (AzCallbackType)on_startup, .ctx = { .Some = { .tag = AzOptionRefAny_Tag_Some, .payload = AzRefAny_clone(&data) } } };
    window.create_callback = (AzOptionCallback){ .Some = { .tag = AzOptionCallback_Tag_Some, .payload = on_create_cb } };
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
