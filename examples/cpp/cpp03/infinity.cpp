// g++ -std=c++03 -o infinity infinity.cpp -lazul
// Note: This example is simplified - full IFrame scrolling requires more complex setup

#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct InfinityState {
    int file_count;
    int visible_start;
    int visible_count;
};
AZ_REFLECT(InfinityState);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const InfinityState* d = InfinityState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    char title_buf[64];
    std::snprintf(title_buf, 64, "Infinite Gallery - %d images", d->file_count);
    
    Dom title = Dom::create_text(String(title_buf));
    title.set_inline_style(String("font-size: 20px; margin-bottom: 10px;"));
    
    Dom container = Dom::create_div();
    container.set_inline_style(String("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px; flex-grow: 1; overflow: scroll; background: #f5f5f5;"));
    
    int end = d->visible_start + d->visible_count;
    if (end > d->file_count) end = d->file_count;
    
    for (int i = d->visible_start; i < end; ++i) {
        char item_buf[32];
        std::snprintf(item_buf, 32, "image_%04d.png", i);
        
        Dom item = Dom::create_div();
        item.set_inline_style(String("width: 150px; height: 150px; background: white; display: flex; align-items: center; justify-content: center;"));
        item.add_child(Dom::create_text(String(item_buf)));
        container.add_child(item);
    }
    
    Dom body = Dom::create_body();
    body.set_inline_style(String("padding: 20px; font-family: sans-serif;"));
    body.add_child(title);
    body.add_child(container);
    
    return body.style(Css::empty()).release();
}

int main() {
    InfinityState state;
    state.file_count = 1000;
    state.visible_start = 0;
    state.visible_count = 20;
    RefAny data = InfinityState_upcast(state);
    
    LayoutCallback cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(cb);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Infinite Scrolling Gallery", 0, 27);
    window.inner().window_state.size.dimensions.width = 800.0;
    window.inner().window_state.size.dimensions.height = 600.0;
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
