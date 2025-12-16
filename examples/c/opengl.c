// OpenGL Integration - C
// cc -o opengl opengl.c -lazul

#include <azul.h>
#include <stdio.h>
#include <math.h>

typedef struct {
    float rotation_deg;
    int texture_uploaded;
} OpenGlState;

void OpenGlState_destructor(void* s) { }
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info);
AzUpdate on_startup(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzString title_text = AzString_copyFromBytes((const uint8_t*)"OpenGL Integration Demo", 0, 23);
    AzDom title = AzDom_text(title_text);
    AzString title_style = AzString_copyFromBytes((const uint8_t*)"color: white; font-size: 24px; margin-bottom: 20px;", 0, 52);
    AzDom_setInlineStyle(&title, title_style);
    
    AzDom image = AzDom_image(AzImageRef_callback(AzRefAny_deepCopy(&data), render_texture));
    AzString image_style = AzString_copyFromBytes((const uint8_t*)
        "flex-grow: 1;"
        "min-height: 300px;"
        "border-radius: 10px;"
        "box-shadow: 0px 0px 20px rgba(0,0,0,0.5);", 0, 94);
    AzDom_setInlineStyle(&image, image_style);
    
    AzDom body = AzDom_body();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"background: linear-gradient(#1a1a2e, #16213e); padding: 20px;", 0, 62);
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, image);
    
    return AzStyledDom_new(body, AzCss_empty());
}

AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info) {
    AzPhysicalSizeU32 size = AzRenderImageCallbackInfo_getBounds(&info).physical_size;
    
    OpenGlStateRef d = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d)) {
        AzVec_u8 empty = AzVec_u8_new();
        return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_RGBA8, empty);
    }
    
    AzGlContextPtr gl_context = AzRenderImageCallbackInfo_getGlContext(&info);
    if (!gl_context.ptr) {
        AzVec_u8 empty = AzVec_u8_new();
        OpenGlStateRef_delete(&d);
        return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_RGBA8, empty);
    }
    
    float rotation = d.ptr->rotation_deg;
    OpenGlStateRef_delete(&d);
    
    AzTexture texture = AzTexture_allocateRgba8(gl_context, size, AzColorU_fromStr("#1a1a2e"));
    AzTexture_clear(&texture);
    
    // Draw rotating rectangles
    AzLogicalRect rect1 = { .x = 100, .y = 100, .width = 200, .height = 200 };
    AzVec_StyleTransform transforms1 = AzVec_StyleTransform_new();
    AzVec_StyleTransform_push(&transforms1, AzStyleTransform_Rotate(AzAngleValue_deg(rotation)));
    AzTexture_drawRect(&texture, rect1, AzColorU_fromStr("#e94560"), transforms1);
    
    AzLogicalRect rect2 = { .x = 150, .y = 150, .width = 100, .height = 100 };
    AzVec_StyleTransform transforms2 = AzVec_StyleTransform_new();
    AzVec_StyleTransform_push(&transforms2, AzStyleTransform_Rotate(AzAngleValue_deg(-rotation * 2)));
    AzTexture_drawRect(&texture, rect2, AzColorU_fromStr("#0f3460"), transforms2);
    
    return AzImageRef_glTexture(texture);
}

AzUpdate on_startup(AzRefAny data, AzCallbackInfo info) {
    AzDuration interval = { .System = { .tag = AzDuration_Tag_System, .payload = AzSystemTimeDiff_fromMillis(16) } };
    AzTimer timer = AzTimer_new(AzRefAny_deepCopy(&data), animate, AzCallbackInfo_getSystemTimeFn(&info));
    timer = AzTimer_withInterval(timer, interval);
    AzTimerId timer_id = { .id = 1 };
    AzCallbackInfo_addTimer(&info, timer_id, timer);
    return AzUpdate_DoNothing;
}

AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info) {
    OpenGlStateRefMut d = OpenGlStateRefMut_create(&data);
    if (!OpenGlState_downcastMut(&data, &d)) {
        return (AzTimerCallbackReturn){ .should_update = AzUpdate_DoNothing };
    }
    
    d.ptr->rotation_deg += 1.0f;
    if (d.ptr->rotation_deg >= 360.0f) {
        d.ptr->rotation_deg = 0.0f;
    }
    OpenGlStateRefMut_delete(&d);
    
    return (AzTimerCallbackReturn){ .should_update = AzUpdate_RefreshDom };
}

int main() {
    OpenGlState state = { .rotation_deg = 0.0f, .texture_uploaded = 0 };
    AzRefAny data = OpenGlState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("OpenGL Integration");
    window.state.size.dimensions.width = 800.0;
    window.state.size.dimensions.height = 600.0;
    AzWindowCreateOptions_setOnCreate(&window, AzRefAny_clone(&data), on_startup);
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
