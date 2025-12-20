// Infinite Scrolling - C
// cc -o infinity infinity.c -lazul

#include <azul.h>
#include <stdio.h>
#include <string.h>

#define MAX_FILES 1000
#define MAX_VISIBLE 20
#define ITEM_HEIGHT 160.0f
#define ITEMS_PER_ROW 4

typedef struct {
    char file_paths[MAX_FILES][256];
    size_t file_count;
} InfinityState;

void InfinityState_destructor(void* s) { }
AZ_REFLECT(InfinityState, InfinityState_destructor);

AzIFrameCallbackReturn render_iframe(AzRefAny data, AzIFrameCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    InfinityStateRef d = InfinityStateRef_create(&data);
    if (!InfinityState_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    
    char title_buf[64];
    snprintf(title_buf, sizeof(title_buf), "Infinite Gallery - %zu images", d.ptr->file_count);
    InfinityStateRef_delete(&d);
    
    AzString title_text = AzString_copyFromBytes((const uint8_t*)title_buf, 0, strlen(title_buf));
    AzDom title = AzDom_createText(title_text);
    AzString title_style = AzString_copyFromBytes((const uint8_t*)"font-size: 20px; margin-bottom: 10px;", 0, 38);
    AzDom_setInlineStyle(&title, title_style);
    
    AzIFrameCallback iframe_cb = { .cb = render_iframe };
    AzDom iframe = AzDom_createIframe(AzRefAny_deepCopy(&data), iframe_cb);
    AzString iframe_style = AzString_copyFromBytes((const uint8_t*)"flex-grow: 1; overflow: scroll; background: #f5f5f5;", 0, 53);
    AzDom_setInlineStyle(&iframe, iframe_style);
    
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"padding: 20px; font-family: sans-serif;", 0, 40);
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, iframe);
    
    return AzDom_style(&body, AzCss_empty());
}

AzIFrameCallbackReturn render_iframe(AzRefAny data, AzIFrameCallbackInfo info) {
    InfinityStateRef d = InfinityStateRef_create(&data);
    if (!InfinityState_downcastRef(&data, &d)) {
        AzLogicalSize zero_size = AzLogicalSize_zero();
        AzLogicalPosition zero_pos = AzLogicalPosition_zero();
        return AzIFrameCallbackReturn_withDom(AzStyledDom_default(), zero_size, zero_pos, zero_size, zero_pos);
    }
    
    size_t file_count = d.ptr->file_count;
    
    // Calculate which items to render based on scroll position
    float scroll_y = info.scroll_offset.y;
    size_t first_row = (size_t)(scroll_y / ITEM_HEIGHT);
    size_t visible_start = first_row * ITEMS_PER_ROW;
    size_t visible_count = MAX_VISIBLE;
    
    if (visible_start > file_count) visible_start = file_count;
    
    size_t end = visible_start + visible_count;
    if (end > file_count) end = file_count;
    
    AzDom container = AzDom_createDiv();
    AzString container_style = AzString_copyFromBytes((const uint8_t*)"display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;", 0, 58);
    AzDom_setInlineStyle(&container, container_style);
    
    for (size_t i = visible_start; i < end; i++) {
        AzDom item = AzDom_createDiv();
        AzString item_style = AzString_copyFromBytes((const uint8_t*)"width: 150px; height: 150px; background: white; border: 1px solid #ddd;", 0, 72);
        AzDom_setInlineStyle(&item, item_style);
        AzString item_text = AzString_copyFromBytes((const uint8_t*)d.ptr->file_paths[i], 0, strlen(d.ptr->file_paths[i]));
        AzDom_addChild(&item, AzDom_createText(item_text));
        AzDom_addChild(&container, item);
    }
    
    InfinityStateRef_delete(&d);
    
    AzStyledDom dom = AzDom_style(&container, AzCss_empty());
    
    // Calculate actual rendered size
    size_t rows_rendered = ((end - visible_start) + ITEMS_PER_ROW - 1) / ITEMS_PER_ROW;
    AzLogicalSize scroll_size = AzLogicalSize_new(800.0f, (float)rows_rendered * ITEM_HEIGHT);
    AzLogicalPosition scroll_offset = AzLogicalPosition_new(0.0f, (float)first_row * ITEM_HEIGHT);
    
    // Calculate virtual (total) size
    size_t total_rows = (file_count + ITEMS_PER_ROW - 1) / ITEMS_PER_ROW;
    AzLogicalSize virtual_scroll_size = AzLogicalSize_new(800.0f, (float)total_rows * ITEM_HEIGHT);
    AzLogicalPosition virtual_scroll_offset = AzLogicalPosition_zero();
    
    return AzIFrameCallbackReturn_withDom(dom, scroll_size, scroll_offset, virtual_scroll_size, virtual_scroll_offset);
}

int main() {
    InfinityState state;
    memset(&state, 0, sizeof(state));
    state.file_count = MAX_FILES;
    
    // Generate dummy file names
    for (int i = 0; i < MAX_FILES; i++) {
        snprintf(state.file_paths[i], sizeof(state.file_paths[i]), "image_%04d.png", i);
    }
    
    AzRefAny data = InfinityState_upcast(state);
    
    AzLayoutCallback layout_cb = { .cb = layout };
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_cb);
    window.window_state.title = AzString_copyFromBytes((const uint8_t*)"Infinite Scrolling Gallery", 0, 27);
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    return 0;
}
