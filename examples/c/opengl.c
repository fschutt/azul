// OpenGL Integration - C
// cc -o opengl opengl.c -lazul

#include <azul.h>
#include <stdio.h>
#include <math.h>

typedef struct {
    float rotation_deg;
    int texture_uploaded;
} OpenGlState;

void OpenGlState_destructor(OpenGlState* s) { }
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

AzImageRef render_texture(AzRefAny* data, AzRenderImageCallbackInfo* info);
AzUpdate on_startup(AzRefAny* data, AzCallbackInfo* info);
AzUpdate animate(AzRefAny* data, AzTimerCallbackInfo* info);

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    AzDom title = AzDom_text(AzString_fromConstStr("OpenGL Integration Demo"));
    AzDom_setInlineStyle(&title, AzString_fromConstStr("color: white; font-size: 24px; margin-bottom: 20px;"));
    
    AzDom image = AzDom_image(AzImageRef_callback(AzRefAny_clone(data), render_texture));
    AzDom_setInlineStyle(&image, AzString_fromConstStr(
        "flex-grow: 1;"
        "min-height: 300px;"
        "border-radius: 10px;"
        "box-shadow: 0px 0px 20px rgba(0,0,0,0.5);"));
    
    AzDom body = AzDom_body();
    AzDom_setInlineStyle(&body, AzString_fromConstStr("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;"));
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, image);
    
    return AzStyledDom_new(body, AzCss_empty());
}

AzImageRef render_texture(AzRefAny* data, AzRenderImageCallbackInfo* info) {
    AzPhysicalSizeU32 size = AzRenderImageCallbackInfo_getBounds(info).physical_size;
    
    OpenGlStateRef d = OpenGlStateRef_create(data);
    if (!OpenGlState_downcastRef(data, &d)) {
        AzVec_u8 empty = AzVec_u8_new();
        return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_RGBA8, empty);
    }
    
    AzGlContextPtr gl_context = AzRenderImageCallbackInfo_getGlContext(info);
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

AzUpdate on_startup(AzRefAny* data, AzCallbackInfo* info) {
    AzTimer timer = AzTimer_new(AzRefAny_clone(data), animate, AzCallbackInfo_getSystemTimeFn(info));
    AzTimer_setInterval(&timer, AzDuration_milliseconds(16));
    AzCallbackInfo_startTimer(info, timer);
    return AzUpdate_DoNothing;
}

AzUpdate animate(AzRefAny* data, AzTimerCallbackInfo* info) {
    OpenGlStateRefMut d = OpenGlStateRefMut_create(data);
    if (!OpenGlState_downcastMut(data, &d)) {
        return AzUpdate_DoNothing;
    }
    
    d.ptr->rotation_deg += 1.0f;
    if (d.ptr->rotation_deg >= 360.0f) {
        d.ptr->rotation_deg = 0.0f;
    }
    OpenGlStateRefMut_delete(&d);
    
    return AzUpdate_RefreshDom;
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
