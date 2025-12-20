// g++ -std=c++11 -o infinity infinity.cpp -lazul
// Note: This example is simplified - full IFrame scrolling requires more complex setup

#include "azul23.hpp"
#include <vector>
#include <string>
#include <sstream>
#include <iomanip>

using namespace azul;

struct InfinityState {
    std::vector<std::string> file_paths;
    size_t visible_start;
    size_t visible_count;
    
    InfinityState() : visible_start(0), visible_count(20) {}
};
AZ_REFLECT(InfinityState);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const InfinityState* d = InfinityState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    std::ostringstream title_text;
    title_text << "Infinite Gallery - " << d->file_paths.size() << " images";
    
    Dom title = Dom::create_text(title_text.str().c_str());
    title.set_inline_style("font-size: 20px; margin-bottom: 10px;");
    
    Dom container = Dom::create_div();
    container.set_inline_style("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px; flex-grow: 1; overflow: scroll; background: #f5f5f5;");
    
    size_t end = std::min(d->visible_start + d->visible_count, d->file_paths.size());
    for (size_t i = d->visible_start; i < end; ++i) {
        Dom item = Dom::create_div();
        item.set_inline_style("width: 150px; height: 150px; background: white; display: flex; align-items: center; justify-content: center;");
        item.add_child(Dom::create_text(d->file_paths[i].c_str()));
        container.add_child(std::move(item));
    }
    
    Dom body = Dom::create_body();
    body.set_inline_style("padding: 20px; font-family: sans-serif;");
    body.add_child(std::move(title));
    body.add_child(std::move(container));
    
    return body.style(Css::empty()).release();
}

int main() {
    InfinityState state;
    for (int i = 0; i < 1000; ++i) {
        std::ostringstream filename;
        filename << "image_" << std::setw(4) << std::setfill('0') << i << ".png";
        state.file_paths.push_back(filename.str());
    }
    
    RefAny data = InfinityState_upcast(state);
    LayoutCallback cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(std::move(cb));
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Infinite Scrolling Gallery", 0, 27);
    window.inner().window_state.size.dimensions.width = 800.0;
    window.inner().window_state.size.dimensions.height = 600.0;
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
