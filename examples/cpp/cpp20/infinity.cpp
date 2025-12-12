// Infinite Scrolling - C++20
// g++ -std=c++20 -o infinity infinity.cpp -lazul

#include <azul.hpp>
#include <format>
#include <vector>
#include <filesystem>

using namespace azul;
using namespace std::string_view_literals;

struct InfinityState {
    std::vector<std::string> file_paths;
    size_t visible_start = 0;
    size_t visible_count = 20;
};

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info);
Update on_scroll(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = InfinityState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto title = Dom::text(std::format("Infinite Gallery - {} images"sv, d->file_paths.size()))
        .with_inline_style("font-size: 20px; margin-bottom: 10px;"sv);
    
    auto iframe = Dom::iframe(data.clone(), render_iframe)
        .with_inline_style("flex-grow: 1; overflow: scroll; background: #f5f5f5;"sv)
        .with_callback(On::Scroll, data.clone(), on_scroll);
    
    auto body = Dom::body()
        .with_inline_style("padding: 20px; font-family: sans-serif;"sv)
        .with_child(title)
        .with_child(iframe);
    
    return body.style(Css::empty());
}

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info) {
    auto d = InfinityState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto container = Dom::div()
        .with_inline_style("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;"sv);
    
    size_t end = std::min(d->visible_start + d->visible_count, d->file_paths.size());
    for (size_t i = d->visible_start; i < end; ++i) {
        auto item = Dom::div()
            .with_inline_style("width: 150px; height: 150px; background: white; border: 1px solid #ddd;"sv)
            .with_child(Dom::text(std::filesystem::path(d->file_paths[i]).filename().string()));
        container.add_child(item);
    }
    
    return container.style(Css::empty());
}

Update on_scroll(RefAny& data, CallbackInfo& info) {
    auto d = InfinityState::downcast_mut(data);
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
        state.file_paths.push_back(std::format("image_{:04d}.png", i));
    }
    
    auto data = RefAny::new(std::move(state));
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Infinite Scrolling Gallery"sv);
    window.set_size(LogicalSize(800, 600));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
