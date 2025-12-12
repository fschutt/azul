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

void InfinityState_destructor(InfinityState* s) { }
AZ_REFLECT(InfinityState, InfinityState_destructor);

AzStyledDom render_iframe(AzRefAny* data, AzIFrameCallbackInfo* info);
AzUpdate on_scroll(AzRefAny* data, AzCallbackInfo* info);

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    InfinityStateRef d = InfinityStateRef_create(data);
    if (!InfinityState_downcastRef(data, &d)) {
        return AzStyledDom_default();
    }
    
    char title_buf[64];
    snprintf(title_buf, sizeof(title_buf), "Infinite Gallery - %zu images", d.ptr->file_count);
    InfinityStateRef_delete(&d);
    
    AzDom title = AzDom_text(AzString_fromConstStr(title_buf));
    AzDom_setInlineStyle(&title, AzString_fromConstStr("font-size: 20px; margin-bottom: 10px;"));
    
    AzDom iframe = AzDom_iframe(AzRefAny_clone(data), render_iframe);
    AzDom_setInlineStyle(&iframe, AzString_fromConstStr("flex-grow: 1; overflow: scroll; background: #f5f5f5;"));
    AzDom_setCallback(&iframe, AzOn_Scroll, AzRefAny_clone(data), on_scroll);
    
    AzDom body = AzDom_body();
    AzDom_setInlineStyle(&body, AzString_fromConstStr("padding: 20px; font-family: sans-serif;"));
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, iframe);
    
    return AzStyledDom_new(body, AzCss_empty());
}

AzStyledDom render_iframe(AzRefAny* data, AzIFrameCallbackInfo* info) {
    InfinityStateRef d = InfinityStateRef_create(data);
    if (!InfinityState_downcastRef(data, &d)) {
        return AzStyledDom_default();
    }
    
    AzDom container = AzDom_div();
    AzDom_setInlineStyle(&container, AzString_fromConstStr("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;"));
    
    size_t end = d.ptr->visible_start + d.ptr->visible_count;
    if (end > d.ptr->file_count) end = d.ptr->file_count;
    
    for (size_t i = d.ptr->visible_start; i < end; i++) {
        AzDom item = AzDom_div();
        AzDom_setInlineStyle(&item, AzString_fromConstStr("width: 150px; height: 150px; background: white; border: 1px solid #ddd;"));
        AzDom_addChild(&item, AzDom_text(AzString_fromConstStr(d.ptr->file_paths[i])));
        AzDom_addChild(&container, item);
    }
    
    InfinityStateRef_delete(&d);
    
    return AzStyledDom_new(container, AzCss_empty());
}

AzUpdate on_scroll(AzRefAny* data, AzCallbackInfo* info) {
    InfinityStateRefMut d = InfinityStateRefMut_create(data);
    if (!InfinityState_downcastMut(data, &d)) {
        return AzUpdate_DoNothing;
    }
    
    AzScrollPosition scroll_pos;
    if (!AzCallbackInfo_getScrollPosition(info, &scroll_pos)) {
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
