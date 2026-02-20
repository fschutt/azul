// OpenGL Integration - C
// Renders animated map data from GeoJSON using OpenGL textures
// cc -o opengl opengl.c -L../../target/release -lazul

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>

// Helper to create AzString from C string
static AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// Application state - mirrors the Rust OpenGlAppState
typedef struct {
    float rotation_deg;
    // Tessellated vertices (CPU side, uploaded on startup)
    AzTessellatedSvgNode fill_vertices;
    AzTessellatedSvgNode stroke_vertices;
    bool vertices_ready;
    // GPU vertex buffers (uploaded after GL context available)
    AzTessellatedGPUSvgNode fill_gpu_node;
    AzTessellatedGPUSvgNode stroke_gpu_node;
    bool gpu_ready;
} OpenGlState;

void OpenGlState_destructor(void* s) {
    // GPU nodes cleaned up when GL context destroyed
}
AZ_REFLECT(OpenGlState, OpenGlState_destructor);

// Forward declarations
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzImageRef render_my_texture(AzRefAny data, AzRenderImageCallbackInfo info);
AzUpdate startup_window(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info);

// Dynamic array for collecting path elements before converting to Vec
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

// Parse multipolygons from JSON - mirrors parse_multipolygons() in Rust
bool parse_and_tessellate(OpenGlState* state) {
    printf("Reading testdata.json...\n");
    
    // Read the JSON file
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
    
    // Prepare tessellation styles
    AzSvgFillStyle fill_style = AzSvgFillStyle_default();
    AzSvgStrokeStyle stroke_style = AzSvgStrokeStyle_default();
    stroke_style.line_width = 4.0f;
    
    // Collect tessellated nodes
    TessNodeArray fill_nodes = {0};
    TessNodeArray stroke_nodes = {0};
    
    // Process each multipolygon (like Rust: parsed.iter().map(...))
    size_t max_polygons = arr_len < 100 ? arr_len : 100;
    for (size_t i = 0; i < max_polygons; i++) {
        AzOptionJson item_opt = AzJson_getIndex(&json, i);
        if (item_opt.Some.tag != AzOptionJson_Tag_Some) continue;
        AzJson item = item_opt.Some.payload;
        
        // Get coordinates array
        AzOptionJson coords_opt = AzJson_getKey(&item, az_str("coordinates"));
        if (coords_opt.Some.tag != AzOptionJson_Tag_Some) {
            AzJson_delete(&item);
            continue;
        }
        AzJson coords = coords_opt.Some.payload;
        
        // coords[0] is the polygon (like Rust: p.coordinates[0])
        AzOptionJson poly_opt = AzJson_getIndex(&coords, 0);
        if (poly_opt.Some.tag != AzOptionJson_Tag_Some) {
            AzJson_delete(&coords);
            AzJson_delete(&item);
            continue;
        }
        AzJson poly = poly_opt.Some.payload;
        
        // Collect rings for this multipolygon
        PathArray rings = {0};
        size_t ring_count = AzJson_len(&poly);
        
        for (size_t r = 0; r < ring_count; r++) {
            AzOptionJson ring_opt = AzJson_getIndex(&poly, r);
            if (ring_opt.Some.tag != AzOptionJson_Tag_Some) continue;
            AzJson ring = ring_opt.Some.payload;
            
            // Collect path elements (like Rust: r.iter().filter_map(...))
            PathElementArray path_elements = {0};
            AzSvgPoint last_point = {0};
            bool has_last = false;
            
            size_t point_count = AzJson_len(&ring);
            for (size_t p = 0; p < point_count; p++) {
                AzOptionJson pt_opt = AzJson_getIndex(&ring, p);
                if (pt_opt.Some.tag != AzOptionJson_Tag_Some) continue;
                AzJson pt = pt_opt.Some.payload;
                
                // pt[0] = x, pt[1] = y
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
                
                // Transform coordinates (exactly like Rust example)
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
                
                // Like Rust: filter_map with last_point logic
                if (has_last) {
                    AzSvgLine line = { .start = last_point, .end = current };
                    path_elem_push(&path_elements, AzSvgPathElement_line(line));
                }
                
                last_point = current;
                has_last = true;
            }
            
            AzJson_delete(&ring);
            
            // Create SvgPath from elements (like Rust: SvgPath { items: ... })
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
        
        // Create SvgMultiPolygon and tessellate (like Rust example)
        if (rings.len > 0) {
            AzSvgPathVec rings_vec = AzSvgPathVec_copyFromPtr(rings.items, rings.len);
            AzSvgMultiPolygon mp = AzSvgMultiPolygon_create(rings_vec);
            
            // Tessellate fill (like Rust: mp.tessellate_fill(SvgFillStyle::default()))
            AzTessellatedSvgNode fill_node = AzSvgMultiPolygon_tessellateFill(&mp, fill_style);
            tess_push(&fill_nodes, fill_node);
            
            // Tessellate stroke (like Rust: mp.tessellate_stroke(stroke_style))
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
    
    // Join all tessellated nodes (like Rust: TessellatedSvgNode::from_nodes(...))
    AzTessellatedSvgNodeVecRef fill_ref = { .ptr = fill_nodes.items, .len = fill_nodes.len };
    AzTessellatedSvgNodeVecRef stroke_ref = { .ptr = stroke_nodes.items, .len = stroke_nodes.len };
    
    state->fill_vertices = AzTessellatedSvgNode_fromNodes(fill_ref);
    state->stroke_vertices = AzTessellatedSvgNode_fromNodes(stroke_ref);
    state->vertices_ready = true;
    
    free(fill_nodes.items);
    free(stroke_nodes.items);
    
    return true;
}

// Layout function - mirrors layout() in Rust
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Create body with gradient background (like Rust example)
    AzDom body = AzDom_createBody();
    AzDom_setInlineStyle(&body, az_str(
        "display: flex;"
        "flex-direction: column;"
        "background: linear-gradient(blue, black);"
        "padding: 10px;"
        "width: 100%;"
        "height: 100%;"
        "box-sizing: border-box;"
    ));
    
    // Create OpenGL image with callback (like Rust: ImageRef::callback(...))
    AzCoreRenderImageCallback callback = { 
        .cb = (AzCoreRenderImageCallbackType)render_my_texture,
        .ctx = { .None = { .tag = AzOptionRefAny_Tag_None } }
    };
    AzImageRef image_ref = AzImageRef_callback(callback, AzRefAny_clone(&data));
    
    AzDom image = AzDom_createImage(image_ref);
    AzDom_setInlineStyle(&image, az_str(
        "flex-grow: 1;"
        "width: 100%;"
        "border: 5px solid red;"
        "border-radius: 50px;"
        "box-sizing: border-box;"
        "box-shadow: 0px 0px 10px black;"
    ));
    
    // Button on top using proper Button widget (like Rust: Button::create("...").dom())
    AzButton button = AzButton_create(az_str("Button composited over OpenGL content!"));
    AzDom button_dom = AzButton_dom(button);
    AzDom_setInlineStyle(&button_dom, az_str(
        "margin-top: 50px;"
        "margin-left: 50px;"
    ));
    AzDom_addChild(&image, button_dom);
    
    AzDom_addChild(&body, image);
    
    return AzDom_style(&body, AzCss_empty());
}

// Render texture callback - mirrors render_my_texture() in Rust
AzImageRef render_my_texture(AzRefAny data, AzRenderImageCallbackInfo info) {
    AzHidpiAdjustedBounds bounds = AzRenderImageCallbackInfo_getBounds(&info);
    AzPhysicalSizeU32 size = AzHidpiAdjustedBounds_getPhysicalSize(&bounds);
    
    // Invalid/null image for error cases - use non-NULL pointer for empty slice
    static uint8_t dummy_byte = 0;
    AzU8VecRef empty = { .ptr = &dummy_byte, .len = 0 };
    AzImageRef invalid = AzImageRef_nullImage(size.width, size.height, AzRawImageFormat_R8, empty);
    
    // Get GL context
    AzOptionGlContextPtr opt_gl = AzRenderImageCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        return invalid;
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    // Downcast state
    OpenGlStateRef d = OpenGlStateRef_create(&data);
    if (!OpenGlState_downcastRef(&data, &d)) {
        return invalid;
    }
    
    float rotation_deg = d.ptr->rotation_deg;
    bool gpu_ready = d.ptr->gpu_ready;
    
    if (!gpu_ready) {
        OpenGlStateRef_delete(&d);
        // Return a simple colored texture while waiting for GPU upload
        AzColorU bg_color = AzColorU_red();
        AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
        AzTexture_clear(&texture);
        return AzImageRef_glTexture(texture);
    }
    
    // Allocate texture (like Rust: Texture::allocate_rgba8(...))
    AzColorU bg_color = AzColorU_transparent();
    AzTexture texture = AzTexture_allocateRgba8(gl_context, size, bg_color);
    AzTexture_clear(&texture);
    
    // Draw fill (like Rust: texture.draw_tesselated_svg_gpu_node(...))
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
    
    // Draw stroke
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
    
    return AzImageRef_glTexture(texture);
}

// Window startup callback - mirrors startup_window() in Rust
AzUpdate startup_window(AzRefAny data, AzCallbackInfo info) {
    // Get GL context
    AzOptionGlContextPtr opt_gl = AzCallbackInfo_getGlContext(&info);
    if (opt_gl.Some.tag != AzOptionGlContextPtr_Tag_Some) {
        return AzUpdate_DoNothing;
    }
    AzGlContextPtr gl_context = opt_gl.Some.payload;
    
    // Downcast and upload vertices to GPU
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
    
    // Upload to GPU (like Rust: TessellatedGPUSvgNode::new(...))
    d.ptr->fill_gpu_node = AzTessellatedGPUSvgNode_create(d.ptr->fill_vertices, gl_context);
    d.ptr->stroke_gpu_node = AzTessellatedGPUSvgNode_create(d.ptr->stroke_vertices, gl_context);
    d.ptr->gpu_ready = true;
    
    printf("Uploaded vertices to GPU\n");
    
    OpenGlStateRefMut_delete(&d);
    
    // Add timer for animation (like Rust: info.add_timer(...))
    AzTimerId timer_id = AzTimerId_unique();
    AzGetSystemTimeCallback time_fn = AzCallbackInfo_getSystemTimeFn(&info);
    AzTimer timer = AzTimer_create(AzRefAny_clone(&data), (AzTimerCallbackType)animate, time_fn);
    
    AzSystemTimeDiff interval = AzSystemTimeDiff_fromMillis(16);
    AzDuration duration = { .System = { .tag = AzDuration_Tag_System, .payload = interval } };
    timer = AzTimer_withInterval(timer, duration);
    
    AzCallbackInfo_addTimer(&info, timer_id, timer);
    
    return AzUpdate_RefreshDom;
}

// Animation callback - mirrors animate() in Rust
// Uses updateAllImageCallbacks instead of full DOM rebuild for efficiency
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
    
    // Only re-render image callbacks (OpenGL textures), no DOM rebuild needed
    AzTimerCallbackInfo_updateAllImageCallbacks(&info);
    return AzTimerCallbackReturn_continueUnchanged();
}

int main(void) {
    printf("Starting!\n");
    
    // Initialize state
    OpenGlState state = {
        .rotation_deg = 0.0f,
        .fill_vertices = AzTessellatedSvgNode_empty(),
        .stroke_vertices = AzTessellatedSvgNode_empty(),
        .vertices_ready = false,
        .gpu_ready = false
    };
    
    // Parse and tessellate (like Rust: parse_multipolygons(DATA))
    if (!parse_and_tessellate(&state)) {
        printf("Failed to parse and tessellate\n");
        return 1;
    }
    
    printf("Starting app\n");
    
    // Create app (like Rust: App::create(data, AppConfig::create()))
    AzRefAny data = OpenGlState_upcast(state);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    
    // Create window (like Rust: WindowCreateOptions::create(layout))
    AzWindowCreateOptions window = AzWindowCreateOptions_create((AzLayoutCallbackType)layout);
    window.window_state.title = az_str("OpenGL Integration");
    window.window_state.flags.frame = AzWindowFrame_Maximized;
    
    // Set create callback (like Rust: window.create_callback = Some(...))
    AzCallback create_cb = { 
        .cb = (AzCallbackType)startup_window, 
        .ctx = AzOptionRefAny_some(AzRefAny_clone(&data))
    };
    window.create_callback = AzOptionCallback_some(create_cb);
    
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
