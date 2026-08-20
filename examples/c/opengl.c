#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>

static AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

typedef struct {
    float rotation_deg;
    AzTessellatedSvgNode fill_vertices;
    AzTessellatedSvgNode stroke_vertices;
    bool vertices_ready;
    AzTessellatedGPUSvgNode fill_gpu_node;
    AzTessellatedGPUSvgNode stroke_gpu_node;
    bool gpu_ready;
} OpenGlState;

void OpenGlState_destructor(void* s) {
    // GPU nodes cleaned up when GL context destroyed
}
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzImageRef render_my_texture(AzRefAny data, AzRenderImageCallbackInfo info);
AzUpdate startup_window(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info);

typedef struct {
    AzSvgPathElement* items;
    size_t len;
    size_t cap;
} PathElementArray;

static void path_elem_push(PathElementArray* arr, AzSvgPathElement elem) {
    if (arr->len >= arr->cap) {
        arr->cap = arr->cap == 0 ? 64 : arr->cap * 2;
        arr->items = realloc(arr->items, arr->cap * sizeof(AzSvgPathElement));
    }
    arr->items[arr->len++] = elem;
}

typedef struct {
    AzSvgPath* items;
    size_t len;
    size_t cap;
} PathArray;

static void path_push(PathArray* arr, AzSvgPath path) {
    if (arr->len >= arr->cap) {
        arr->cap = arr->cap == 0 ? 16 : arr->cap * 2;
        arr->items = realloc(arr->items, arr->cap * sizeof(AzSvgPath));
    }
    arr->items[arr->len++] = path;
}

typedef struct {
    AzTessellatedSvgNode* items;
    size_t len;
    size_t cap;
} TessNodeArray;

static void tess_push(TessNodeArray* arr, AzTessellatedSvgNode node) {
    if (arr->len >= arr->cap) {
        arr->cap = arr->cap == 0 ? 64 : arr->cap * 2;
        arr->items = realloc(arr->items, arr->cap * sizeof(AzTessellatedSvgNode));
    }
    arr->items[arr->len++] = node;
}

bool parse_and_tessellate(OpenGlState* state) {
    printf("Reading testdata.json...\n");
    
    AzFilePath path = AzFilePath_create(az_str("../assets/testdata.json"));
    AzResultU8VecFileError result = AzFilePath_readBytes(&path);
    AzFilePath_delete(&path);
    
    if (result.Ok.tag != AzResultU8VecFileError_Tag_Ok) {
        printf("Failed to read testdata.json\n");
        return false;
    }
    
    AzU8Vec bytes = result.Ok.payload;
    printf("Read %zu bytes\n", bytes.len);
    
    AzU8VecRef bytes_ref = { .ptr = bytes.ptr, .len = bytes.len };
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(bytes_ref);
    AzU8Vec_delete(&bytes);
    
    if (parse_result.Ok.tag != AzResultJsonJsonParseError_Tag_Ok) {
        printf("Failed to parse JSON\n");
        return false;
    }
    
    AzJson json = parse_result.Ok.payload;
    size_t arr_len = AzJson_len(&json);
    printf("Found %zu multipolygons\n", arr_len);
    
    if (arr_len == 0) {
        printf("JSON is empty or not an array\n");
        AzJson_delete(&json);
        return false;
    }
    
    AzSvgFillStyle fill_style = AzSvgFillStyle_default();
    AzSvgStrokeStyle stroke_style = AzSvgStrokeStyle_default();
    stroke_style.line_width = 4.0f;
    
    TessNodeArray fill_nodes = {0};
    TessNodeArray stroke_nodes = {0};
    
    size_t max_polygons = arr_len < 100 ? arr_len : 100;
    for (size_t i = 0; i < max_polygons; i++) {
        AzOptionJson item_opt = AzJson_getIndex(&json, i);
        if (item_opt.Some.tag != AzOptionJson_Tag_Some) continue;
        AzJson item = item_opt.Some.payload;
        
        AzOptionJson coords_opt = AzJson_getKey(&item, az_str("coordinates"));
        if (coords_opt.Some.tag != AzOptionJson_Tag_Some) {
            AzJson_delete(&item);
            continue;
        }
        AzJson coords = coords_opt.Some.payload;
        
        AzOptionJson poly_opt = AzJson_getIndex(&coords, 0);
        if (poly_opt.Some.tag != AzOptionJson_Tag_Some) {
            AzJson_delete(&coords);
            AzJson_delete(&item);
            continue;
        }
        AzJson poly = poly_opt.Some.payload;
        
        PathArray rings = {0};
        size_t ring_count = AzJson_len(&poly);
        
        for (size_t r = 0; r < ring_count; r++) {
            AzOptionJson ring_opt = AzJson_getIndex(&poly, r);
            if (ring_opt.Some.tag != AzOptionJson_Tag_Some) continue;
            AzJson ring = ring_opt.Some.payload;
            
            PathElementArray path_elements = {0};
            AzSvgPoint last_point = {0};
            bool has_last = false;
            
            size_t point_count = AzJson_len(&ring);
            for (size_t p = 0; p < point_count; p++) {
                AzOptionJson pt_opt = AzJson_getIndex(&ring, p);
                if (pt_opt.Some.tag != AzOptionJson_Tag_Some) continue;
                AzJson pt = pt_opt.Some.payload;
                
                AzOptionJson x_opt = AzJson_getIndex(&pt, 0);
                AzOptionJson y_opt = AzJson_getIndex(&pt, 1);
                
                if (x_opt.Some.tag != AzOptionJson_Tag_Some || 
                    y_opt.Some.tag != AzOptionJson_Tag_Some) {
                    if (x_opt.Some.tag == AzOptionJson_Tag_Some) 
                        AzJson_delete(&x_opt.Some.payload);
                    if (y_opt.Some.tag == AzOptionJson_Tag_Some) 
                        AzJson_delete(&y_opt.Some.payload);
                    AzJson_delete(&pt);
                    continue;
                }
                
                AzJson x_json = x_opt.Some.payload;
                AzJson y_json = y_opt.Some.payload;
                
                AzOptionF64 x_val = AzJson_asFloat(&x_json);
                AzOptionF64 y_val = AzJson_asFloat(&y_json);
                
                AzJson_delete(&x_json);
                AzJson_delete(&y_json);
                AzJson_delete(&pt);
                
                if (x_val.Some.tag != AzOptionF64_Tag_Some || 
                    y_val.Some.tag != AzOptionF64_Tag_Some) {
                    continue;
                }
                
                float x = (float)x_val.Some.payload;
                float y = (float)y_val.Some.payload;
                x -= 13.804483f;
                y -= 51.05274f;
                x *= 50000.0f;
                y *= 50000.0f;
                x += 700.0f;
                y += 700.0f;
                x *= 2.0f;
                y *= 2.0f;
                
                AzSvgPoint current = { .x = x, .y = y };
                
                if (has_last) {
                    AzSvgLine line = { .start = last_point, .end = current };
                    path_elem_push(&path_elements, AzSvgPathElement_line(line));
                }
                
                last_point = current;
                has_last = true;
            }
            
            AzJson_delete(&ring);
            
            if (path_elements.len > 0) {
                AzSvgPathElementVec elem_vec = AzSvgPathElementVec_copyFromPtr(
                    path_elements.items, path_elements.len);
                AzSvgPath svg_path = AzSvgPath_create(elem_vec);
                path_push(&rings, svg_path);
            }
            free(path_elements.items);
        }
        
        AzJson_delete(&poly);
        AzJson_delete(&coords);
        AzJson_delete(&item);
        
        if (rings.len > 0) {
            AzSvgPathVec rings_vec = AzSvgPathVec_copyFromPtr(rings.items, rings.len);
            AzSvgMultiPolygon mp = AzSvgMultiPolygon_create(rings_vec);
            
            AzTessellatedSvgNode fill_node = AzSvgMultiPolygon_tessellateFill(&mp, fill_style);
            tess_push(&fill_nodes, fill_node);
            
            AzTessellatedSvgNode stroke_node = AzSvgMultiPolygon_tessellateStroke(&mp, stroke_style);
            tess_push(&stroke_nodes, stroke_node);
            
            AzSvgMultiPolygon_delete(&mp);
        }
        free(rings.items);
    }
    
    AzJson_delete(&json);
    
    printf("Tessellated %zu fill nodes and %zu stroke nodes\n", fill_nodes.len, stroke_nodes.len);
    
    if (fill_nodes.len == 0) {
        printf("No polygons tessellated!\n");
        free(fill_nodes.items);
        free(stroke_nodes.items);
        return false;
    }
    
    AzTessellatedSvgNodeVecRef fill_ref = { .ptr = fill_nodes.items, .len = fill_nodes.len };
    AzTessellatedSvgNodeVecRef stroke_ref = { .ptr = stroke_nodes.items, .len = stroke_nodes.len };
    
    state->fill_vertices = AzTessellatedSvgNode_fromNodes(fill_ref);
    state->stroke_vertices = AzTessellatedSvgNode_fromNodes(stroke_ref);
    state->vertices_ready = true;
    
    free(fill_nodes.items);
    free(stroke_nodes.items);
    
    return true;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, az_str(
        "display: flex;"
        "flex-direction: column;"
        "background: linear-gradient(blue, black);"
        "padding: 10px;"
        "width: 100%;"
        "height: 100%;"
        "box-sizing: border-box;"
    ));
    
    AzCoreRenderImageCallback callback = { 
        .cb = (AzCoreRenderImageCallbackType)render_my_texture,
        .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } }
    };
    AzImageRef image_ref = AzImageRef_callback(callback, AzRefAny_clone(&data));
    
    AzDom image = AzDom_createImage(image_ref);
    AzDom_setCss(&image, az_str(
        "flex-grow: 1;"
        "width: 100%;"
        "border: 5px solid red;"
        "border-radius: 50px;"
        "box-sizing: border-box;"
        "box-shadow: 0px 0px 10px black;"
    ));
    
    AzButton button = AzButton_create(az_str("Button composited over OpenGL content!"));
    AzDom button_dom = AzButton_dom(button);
    AzDom_setCss(&button_dom, az_str(
        "margin-top: 50px;"
        "margin-left: 50px;"
    ));
    AzDom_addChild(&image, button_dom);
    
    AzDom_addChild(&body, image);
    
    return body;
}

static AzImageRef null_image(AzPhysicalSizeU32 size) {
    static uint8_t dummy_byte = 0;
    AzU8VecRef empty = { .ptr = &dummy_byte, .len = 0 };
    return AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_R8, empty);
}

AzImageRef render_my_texture(AzRefAny data, AzRenderImageCallbackInfo info) {
    AzHidpiAdjustedBounds bounds = AzRenderImageCallbackInfo_getBounds(&info);
    AzPhysicalSizeU32 size = AzHidpiAdjustedBounds_getPhysicalSize(&bounds);
    
    AzOptionGlContextPtr opt_gl = AzRenderImageCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        return null_image(size);
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    OpenGlStateRef d = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d)) {
        return null_image(size);
    }
    
    float rotation_deg = d.ptr->rotation_deg;
    bool gpu_ready = d.ptr->gpu_ready;
    
    if (!gpu_ready) {
        OpenGlStateRef_delete(&d);
        AzColorU bg_color = AzColorU_red();
        AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
        AzTexture_clear(&texture);
        return AzImageRef_glTexture(texture);
    }
    
    AzColorU bg_color = AzColorU_transparent();
    AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
    AzTexture_clear(&texture);
    
    AzStyleTransform fill_transforms[2];
    AzStyleTransformTranslate2D translate = { 
        .x = AzPixelValue_px(400.0f), 
        .y = AzPixelValue_px(400.0f) 
    };
    fill_transforms[0] = AzStyleTransform_translate(translate);
    fill_transforms[1] = AzStyleTransform_rotate(AzAngleValue_deg(rotation_deg));
    AzStyleTransformVec fill_vec = AzStyleTransformVec_copyFromPtr(fill_transforms, 2);
    
    AzColorU fill_color = AzColorU_magenta();
    AzTessellatedGPUSvgNode_draw(
        &d.ptr->fill_gpu_node,
        &texture,
        size,
        fill_color,
        fill_vec
    );
    
    AzStyleTransform stroke_transforms[1];
    stroke_transforms[0] = AzStyleTransform_rotate(AzAngleValue_deg(rotation_deg));
    AzStyleTransformVec stroke_vec = AzStyleTransformVec_copyFromPtr(stroke_transforms, 1);
    
    AzColorU stroke_color = AzColorU_cyan();
    AzTessellatedGPUSvgNode_draw(
        &d.ptr->stroke_gpu_node,
        &texture,
        size,
        stroke_color,
        stroke_vec
    );
    
    OpenGlStateRef_delete(&d);

    AzTexture_applyFxaa(&texture);

    return AzImageRef_glTexture(texture);
}

AzUpdate startup_window(AzRefAny data, AzCallbackInfo info) {
    AzOptionGlContextPtr opt_gl = AzCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        return AzUpdate_DoNothing;
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    OpenGlStateRefMut d = OpenGlStateRefMut_create(&data);
    if (!OpenGlState_downcastMut(&data, &d)) {
        printf("Failed to downcast on startup\n");
        return AzUpdate_DoNothing;
    }
    
    if (!d.ptr->vertices_ready) {
        printf("Vertices not ready\n");
        OpenGlStateRefMut_delete(&d);
        return AzUpdate_DoNothing;
    }
    
    d.ptr->fill_gpu_node = AzTessellatedGPUSvgNode_create(d.ptr->fill_vertices, gl_context);
    d.ptr->stroke_gpu_node = AzTessellatedGPUSvgNode_create(d.ptr->stroke_vertices, gl_context);
    d.ptr->gpu_ready = true;
    
    printf("Uploaded vertices to GPU\n");
    
    OpenGlStateRefMut_delete(&d);
    
    AzTimerId timer_id = AzTimerId_unique();
    AzGetSystemTimeCallback time_fn = AzCallbackInfo_getSystemTimeFn(&info);
    AzTimer timer = AzTimer_create(AzRefAny_clone(&data), (AzTimerCallback){ .cb = animate, .ctx = AzOptionRefAny_none() }, time_fn);
    
    AzSystemTimeDiff interval = AzSystemTimeDiff_fromMillis(16);
    AzDuration duration = { .System = { .tag = AzDuration_Tag_System, .payload = interval } };
    timer = AzTimer_withInterval(timer, duration);
    
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
    
    AzTimerCallbackInfo_updateAllImageCallbacks(&info);
    return AzTimerCallbackReturn_continueUnchanged();
}

int main(void) {
    printf("Starting!\n");
    
    OpenGlState state = {
        .rotation_deg = 0.0f,
        .fill_vertices = AzTessellatedSvgNode_empty(),
        .stroke_vertices = AzTessellatedSvgNode_empty(),
        .vertices_ready = false,
        .gpu_ready = false
    };
    
    if (!parse_and_tessellate(&state)) {
        printf("Failed to parse and tessellate\n");
        return 1;
    }
    
    printf("Starting app\n");
    
    AzRefAny data = OpenGlState_upcast(state);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create((AzLayoutCallbackType)layout);
    window.window_state.title = az_str("OpenGL Integration");
    window.window_state.flags.frame = AzWindowFrame_Maximized;
    
    AzCallback create_cb = { 
        .cb = (AzCallbackType)startup_window, 
        .ctx = AzOptionRefAny_some(AzRefAny_clone(&data))
    };
    window.create_callback = AzOptionCallback_some(create_cb);
    
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
