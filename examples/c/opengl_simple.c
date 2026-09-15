#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <math.h>

static AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

typedef struct {
    float rotation_deg;
    AzTessellatedSvgNode vertices;
    bool vertices_ready;
    AzTessellatedGPUSvgNode gpu_node;
    bool gpu_ready;
} OpenGlState;

void OpenGlState_destructor(void* s) {
}
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info);
AzUpdate on_startup(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info);

bool create_triangle(OpenGlState* state) {
    AzSvgPoint p1 = { .x = 400.0f, .y = 100.0f };
    AzSvgPoint p2 = { .x = 100.0f, .y = 500.0f };
    AzSvgPoint p3 = { .x = 700.0f, .y = 500.0f };
    
    AzSvgLine line1 = { .start = p1, .end = p2 };
    AzSvgLine line2 = { .start = p2, .end = p3 };
    AzSvgLine line3 = { .start = p3, .end = p1 };
    
    AzSvgPathElement elements[3];
    elements[0] = AzSvgPathElement_line(line1);
    elements[1] = AzSvgPathElement_line(line2);
    elements[2] = AzSvgPathElement_line(line3);
    
    AzSvgPathElementVec path_elements = AzSvgPathElementVec_copyFromPtr(elements, 3);
    
    AzSvgPath svg_path = AzSvgPath_create(path_elements);
    
    AzSvgPathVec rings = AzSvgPathVec_fromItem(svg_path);
    
    AzSvgMultiPolygon mp = AzSvgMultiPolygon_create(rings);
    
    AzSvgFillStyle fill_style = AzSvgFillStyle_default();
    state->vertices = AzSvgMultiPolygon_tessellateFill(&mp, fill_style);
    state->vertices_ready = true;
    
    AzSvgMultiPolygon_delete(&mp);
    
    printf("Created triangle with tessellation\n");
    return true;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzCoreRenderImageCallback callback = { 
        .cb = (AzCoreRenderImageCallbackType)render_texture,
        .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } }
    };
    AzImageRef image_ref = AzImageRef_callback(callback, AzRefAny_clone(&data));
    AzDom image = AzDom_createImage(image_ref);
    AzDom_setCss(&image, az_str(
        "flex-grow: 1;"
        "border-radius: 50px;"
        "box-sizing: border-box;"
        "box-shadow: 0px 0px 10px black;"
    ));
    
    AzDom button = AzDom_createPWithText(az_str("Button drawn on top of OpenGL!"));
    AzDom_setCss(&button, az_str(
        "margin-top: 50px;"
        "margin-left: 50px;"
        "padding: 10px 20px;"
        "background: #0078d4;"
        "color: white;"
        "border-radius: 5px;"
        "font-size: 16px;"
    ));
    AzDom_addChild(&image, button);
    
    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, az_str(
        "background: linear-gradient(blue, black);"
        "padding: 10px;"
    ));
    AzDom_addChild(&body, image);
    
    return body;
}

AzImageRef render_texture(AzRefAny data, AzRenderImageCallbackInfo info) {
    AzHidpiAdjustedBounds bounds = AzRenderImageCallbackInfo_getBounds(&info);
    AzPhysicalSizeU32 size = AzHidpiAdjustedBounds_getPhysicalSize(&bounds);
    
    AzOptionGlContextPtr opt_gl = AzRenderImageCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        AzU8VecRef empty = { .ptr = NULL, .len = 0 };
        return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_R8, empty);
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    OpenGlStateRef d = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d)) {
        AzU8VecRef empty = { .ptr = NULL, .len = 0 };
        return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_R8, empty);
    }
    
    float rotation = d.ptr->rotation_deg;
    bool gpu_ready = d.ptr->gpu_ready;
    OpenGlStateRef_delete(&d);
    
    AzColorU bg_color = AzColorU_fromStr(az_str("#ffffffef"));
    AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
    AzTexture_clear(&texture);
    
    if (!gpu_ready) {
        return AzImageRef_glTexture(texture);
    }
    
    OpenGlStateRef d2 = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d2)) {
        return AzImageRef_glTexture(texture);
    }
    
    AzStyleTransform transforms[1];
    transforms[0] = AzStyleTransform_rotate(AzAngleValue_deg(rotation));
    AzStyleTransformVec transform_vec = AzStyleTransformVec_copyFromPtr(transforms, 1);
    
    AzColorU fill_color = AzColorU_fromStr(az_str("#cc00cc"));
    AzTessellatedGPUSvgNode_draw(
        &d2.ptr->gpu_node,
        &texture,
        size,
        fill_color,
        transform_vec
    );
    
    OpenGlStateRef_delete(&d2);
    
    return AzImageRef_glTexture(texture);
}

AzUpdate on_startup(AzRefAny data, AzCallbackInfo info) {
    AzOptionGlContextPtr opt_gl = AzCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        printf("No GL context available on startup\n");
        return AzUpdate_DoNothing;
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    OpenGlStateRefMut d = OpenGlStateRefMut_create(&data);
    if (!OpenGlState_downcastMut(&data, &d)) {
        printf("Failed to downcast state on startup\n");
        return AzUpdate_DoNothing;
    }
    
    if (!d.ptr->vertices_ready) {
        printf("Vertices not ready yet\n");
        OpenGlStateRefMut_delete(&d);
        return AzUpdate_DoNothing;
    }
    
    d.ptr->gpu_node = AzTessellatedGPUSvgNode_create(d.ptr->vertices, gl_context);
    d.ptr->gpu_ready = true;
    
    printf("Uploaded vertices to GPU\n");
    
    OpenGlStateRefMut_delete(&d);
    
    AzGetSystemTimeCallback time_fn = AzCallbackInfo_getSystemTimeFn(&info);
    AzTimer timer = AzTimer_create(AzRefAny_clone(&data), (AzTimerCallback){ .cb = animate, .ctx = AzOptionRefAny_none() }, time_fn);
    
    AzSystemTimeDiff interval_diff = AzSystemTimeDiff_fromMillis(16);
    AzDuration interval = { .System = { .tag = AzDuration_Tag_System, .payload = interval_diff } };
    timer = AzTimer_withInterval(timer, interval);
    
    AzTimerId timer_id = AzTimerId_unique();
    AzCallbackInfo_addTimer(&info, timer_id, timer);
    
    return AzUpdate_RefreshDom;
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
    
    return AzTimerCallbackReturn_continueAndRefreshDom();
}

int main(void) {
    printf("Simple OpenGL Integration Demo\n");
    
    OpenGlState state = {
        .rotation_deg = 0.0f,
        .vertices = AzTessellatedSvgNode_empty(),
        .vertices_ready = false,
        .gpu_ready = false
    };
    
    if (!create_triangle(&state)) {
        printf("Failed to create triangle\n");
        return 1;
    }
    
    printf("Starting app...\n");
    
    AzRefAny data = OpenGlState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create((AzLayoutCallbackType)layout);
    window.window_state.title = az_str("OpenGL Integration");
    window.window_state.flags.frame = AzWindowFrame_Maximized;
    
    AzCallback on_create_cb = { 
        .cb = (AzCallbackType)on_startup, 
        .ctx = AzOptionRefAny_some(AzRefAny_clone(&data))
    };
    window.create_callback = AzOptionCallback_some(on_create_cb);
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
