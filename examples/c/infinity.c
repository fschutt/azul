// Infinite Scrolling - C
// cc -o infinity infinity.c -lazul

#include <azul.h>
#include <stdio.h>
#include <string.h>

#define MAX_FILES 1000
#define MAX_VISIBLE 20

typedef struct {
    char file_paths[MAX_FILES][256];
    size_t file_count;
    size_t visible_start;
    size_t visible_count;
} InfinityState;

void InfinityState_destructor(void* s) { }
AZ_REFLECT(InfinityState, InfinityState_destructor);

AzIFrameCallbackReturn render_iframe(AzRefAny data, AzIFrameCallbackInfo info);
AzUpdate on_scroll(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    InfinityStateRef d = InfinityStateRef_create(&data);
    if (!InfinityState_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    
    char title_buf[64];
    snprintf(title_buf, sizeof(title_buf), "Infinite Gallery - %zu images", d.ptr->file_count);
    InfinityStateRef_delete(&d);
    
    AzString title_text = AzString_copyFromBytes((const uint8_t*)title_buf, 0, strlen(title_buf));
    AzDom title = AzDom_text(title_text);
    AzString title_style = AzString_copyFromBytes((const uint8_t*)"font-size: 20px; margin-bottom: 10px;", 0, 38);
    AzDom_setInlineStyle(&title, title_style);
    
    AzDom iframe = AzDom_iframe(AzRefAny_deepCopy(&data), render_iframe);
    AzString iframe_style = AzString_copyFromBytes((const uint8_t*)"flex-grow: 1; overflow: scroll; background: #f5f5f5;", 0, 53);
    AzDom_setInlineStyle(&iframe, iframe_style);
    AzEventFilter scroll_event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_Scroll } };
    AzDom_addCallback(&iframe, scroll_event, AzRefAny_deepCopy(&data), on_scroll);
    
    AzDom body = AzDom_body();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"padding: 20px; font-family: sans-serif;", 0, 40);
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, iframe);
    
    return AzStyledDom_new(body, AzCss_empty());
}

AzIFrameCallbackReturn render_iframe(AzRefAny data, AzIFrameCallbackInfo info) {
    InfinityStateRef d = InfinityStateRef_create(&data);
    if (!InfinityState_downcastRef(&data, &d)) {
        return (AzIFrameCallbackReturn){ .dom = { .Some = AzStyledDom_default() }, .scroll_size = {0}, .scroll_offset = {0} };
    }
    
    AzDom container = AzDom_div();
    AzString container_style = AzString_copyFromBytes((const uint8_t*)"display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;", 0, 58);
    AzDom_setInlineStyle(&container, container_style);
    
    size_t end = d.ptr->visible_start + d.ptr->visible_count;
    if (end > d.ptr->file_count) end = d.ptr->file_count;
    
    for (size_t i = d.ptr->visible_start; i < end; i++) {
        AzDom item = AzDom_div();
        AzString item_style = AzString_copyFromBytes((const uint8_t*)"width: 150px; height: 150px; background: white; border: 1px solid #ddd;", 0, 72);
        AzDom_setInlineStyle(&item, item_style);
        AzString item_text = AzString_copyFromBytes((const uint8_t*)d.ptr->file_paths[i], 0, strlen(d.ptr->file_paths[i]));
        AzDom_addChild(&item, AzDom_text(item_text));
        AzDom_addChild(&container, item);
    }
    
    size_t file_count = d.ptr->file_count;
    InfinityStateRef_delete(&d);
    
    AzStyledDom dom = AzStyledDom_new(container, AzCss_empty());
    AzOptionStyledDom some_dom = { .Some = { .tag = AzOptionStyledDom_Tag_Some, .payload = dom } };
    return (AzIFrameCallbackReturn){ .dom = some_dom, .scroll_size = { .width = 800, .height = 160 * ((file_count + 3) / 4) }, .scroll_offset = {0} };
}

AzUpdate on_scroll(AzRefAny data, AzCallbackInfo info) {
    InfinityStateRefMut d = InfinityStateRefMut_create(&data);
    if (!InfinityState_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    
    AzScrollPosition scroll_pos;
    if (!AzCallbackInfo_getScrollPosition(&info, &scroll_pos)) {
        InfinityStateRefMut_delete(&d);
        return AzUpdate_DoNothing;
    }
    
    size_t new_start = (size_t)(scroll_pos.y / 160) * 4;
    if (new_start != d.ptr->visible_start) {
        d.ptr->visible_start = new_start;
        if (d.ptr->visible_start > d.ptr->file_count) {
            d.ptr->visible_start = d.ptr->file_count;
        }
        InfinityStateRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    
    InfinityStateRefMut_delete(&d);
    return AzUpdate_DoNothing;
}

int main() {
    InfinityState state;
    memset(&state, 0, sizeof(state));
    state.visible_start = 0;
    state.visible_count = MAX_VISIBLE;
    state.file_count = MAX_FILES;
    
    // Generate dummy file names
    for (int i = 0; i < MAX_FILES; i++) {
        snprintf(state.file_paths[i], sizeof(state.file_paths[i]), "image_%04d.png", i);
    }
    
    AzRefAny data = InfinityState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Infinite Scrolling Gallery");
    window.state.size.dimensions.width = 800.0;
    window.state.size.dimensions.height = 600.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
