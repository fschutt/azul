// g++ -std=c++03 -o infinity infinity.cpp -lazul

#include <azul.hpp>
#include <vector>
#include <string>
#include <sstream>
#include <iomanip>

using namespace azul;

struct InfinityState {
    std::vector<std::string> file_paths;
    size_t visible_start;
    size_t visible_count;
};

void InfinityState_init(InfinityState* s) {
    s->visible_start = 0;
    s->visible_count = 20;
}

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info);
Update on_scroll(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    InfinityState* d = InfinityState_downcast_ref(data);
    if (!d) return StyledDom_default();
    
    std::ostringstream title_text;
    title_text << "Infinite Gallery - " << d->file_paths.size() << " images";
    
    Dom title = Dom_text(title_text.str().c_str());
    Dom_setInlineStyle(title, "font-size: 20px; margin-bottom: 10px;");
    
    Dom iframe = Dom_iframe(RefAny_clone(data), render_iframe);
    Dom_setInlineStyle(iframe, "
        flex-grow: 1; 
        overflow: scroll; 
        background: #f5f5f5;
    ");
    Dom_setCallback(iframe, On_Scroll, RefAny_clone(data), on_scroll);
    
    Dom body = Dom_body();
    Dom_setInlineStyle(body, "padding: 20px; font-family: sans-serif;");
    Dom_addChild(body, title);
    Dom_addChild(body, iframe);
    
    return StyledDom_new(body, Css_empty());
}

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info) {
    InfinityState* d = InfinityState_downcast_ref(data);
    if (!d) return StyledDom_default();
    
    Dom container = Dom_div();
    Dom_setInlineStyle(container, "
        display: flex; 
        flex-wrap: wrap; 
        gap: 10px; 
        padding: 10px;
    ");
    
    size_t end = d->visible_start + d->visible_count;
    if (end > d->file_paths.size()) end = d->file_paths.size();
    
    for (size_t i = d->visible_start; i < end; ++i) {
        Dom item = Dom_div();
        Dom_setInlineStyle(item, "width: 150px; height: 150px; background: white;");
        Dom_addChild(item, Dom_text(d->file_paths[i].c_str()));
        Dom_addChild(container, item);
    }
    
    return StyledDom_new(container, Css_empty());
}

Update on_scroll(RefAny& data, CallbackInfo& info) {
    InfinityState* d = InfinityState_downcast_mut(data);
    if (!d) return Update_DoNothing;
    
    ScrollPosition scroll_pos;
    if (!CallbackInfo_getScrollPosition(info, &scroll_pos)) return Update_DoNothing;
    
    size_t new_start = (size_t)(scroll_pos.y / 160) * 4;
    if (new_start != d->visible_start) {
        d->visible_start = new_start;
        if (d->visible_start > d->file_paths.size()) {
            d->visible_start = d->file_paths.size();
        }
        return Update_RefreshDom;
    }
    return Update_DoNothing;
}

int main() {
    InfinityState state;
    InfinityState_init(&state);
    
    for (int i = 0; i < 1000; ++i) {
        std::ostringstream filename;
        filename << "image_" << std::setw(4) << std::setfill('0') << i << ".png";
        state.file_paths.push_back(filename.str());
    }
    
    RefAny data = InfinityState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions_new(layout);
    WindowCreateOptions_setTitle(window, "Infinite Scrolling Gallery");
    WindowCreateOptions_setSize(window, LogicalSize_new(800, 600));
    
    App app = App_new(data, AppConfig_default());
    App_run(app, window);
    return 0;
}
