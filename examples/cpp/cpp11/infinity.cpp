// Infinite Scrolling - C++11
// g++ -std=c++11 -o infinity infinity.cpp -lazul

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
    
    InfinityState() : visible_start(0), visible_count(20) {}
};

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info);
Update on_scroll(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    InfinityState* d = InfinityState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    std::ostringstream title_text;
    title_text << "Infinite Gallery - " << d->file_paths.size() << " images";
    
    auto title = Dom::text(title_text.str())
        .with_inline_style("font-size: 20px; margin-bottom: 10px;");
    
    auto iframe = Dom::iframe(data.clone(), render_iframe)
        .with_inline_style("flex-grow: 1; overflow: scroll; background: #f5f5f5;")
        .with_callback(On::Scroll, data.clone(), on_scroll);
    
    auto body = Dom::body()
        .with_inline_style("padding: 20px; font-family: sans-serif;")
        .with_child(title)
        .with_child(iframe);
    
    return body.style(Css::empty());
}

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info) {
    InfinityState* d = InfinityState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto container = Dom::div()
        .with_inline_style("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;");
    
    size_t end = std::min(d->visible_start + d->visible_count, d->file_paths.size());
    for (size_t i = d->visible_start; i < end; ++i) {
        auto item = Dom::div()
            .with_inline_style("width: 150px; height: 150px; background: white;")
            .with_child(Dom::text(d->file_paths[i]));
        container.add_child(item);
    }
    
    return container.style(Css::empty());
}

Update on_scroll(RefAny& data, CallbackInfo& info) {
    InfinityState* d = InfinityState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    auto scroll_pos = info.get_scroll_position();
    if (!scroll_pos) return Update::DoNothing;
    
    size_t new_start = static_cast<size_t>(scroll_pos->y / 160) * 4;
    if (new_start != d->visible_start) {
        d->visible_start = std::min(new_start, d->file_paths.size());
        return Update::RefreshDom;
    }
    return Update::DoNothing;
}

int main() {
    InfinityState state;
    for (int i = 0; i < 1000; ++i) {
        std::ostringstream filename;
        filename << "image_" << std::setw(4) << std::setfill('0') << i << ".png";
        state.file_paths.push_back(filename.str());
    }
    
    auto data = RefAny::new_ref(state);
    auto window = WindowCreateOptions::new_window(layout);
    window.set_title("Infinite Scrolling Gallery");
    window.set_size(LogicalSize(800, 600));
    
    auto app = App::new_app(data, AppConfig::default_config());
    app.run(window);
    return 0;
}
