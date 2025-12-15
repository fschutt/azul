// g++ -std=c++23 -o infinity infinity.cpp -lazul

#include <azul.hpp>
#include <format>
#include <vector>
#include <filesystem>

using namespace azul;
using namespace std::string_view_literals;

struct InfinityState {
    std::vector<std::string> file_paths;
    std::vector<ImageRef> loaded_images;
    size_t visible_start = 0;
    size_t visible_count = 20;
    float scroll_offset = 0.0f;
};

StyledDom render_iframe(RefAny& data, IFrameCallbackInfo& info);
Update on_scroll(RefAny& data, CallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = InfinityState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto title = Dom::text(std::format("Infinite Image Gallery - {} images"sv, d->file_paths.size()))
        .with_inline_style("font-size: 20px; margin-bottom: 10px; color: #333;"sv);
    
    auto scroll_info = Dom::text(std::format("Showing items {} - {} of {}"sv, 
            d->visible_start + 1, 
            std::min(d->visible_start + d->visible_count, d->file_paths.size()),
            d->file_paths.size()))
        .with_inline_style("font-size: 14px; color: #666; margin-bottom: 10px;"sv);
    
    // IFrame for virtualized content
    auto iframe = Dom::iframe(data.clone(), render_iframe)
        .with_inline_style(R"(
            flex-grow: 1;
            overflow: scroll;
            background: #f5f5f5;
            border: 1px solid #ddd;
            border-radius: 5px;
        )"sv)
        .with_callback(On::Scroll, data.clone(), on_scroll);
    
    auto body = Dom::body()
        .with_inline_style("padding: 20px; font-family: sans-serif;"sv)
        .with_child(title)
        .with_child(scroll_info)
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
        const auto& path = d->file_paths[i];
        
        auto item = Dom::div()
            .with_inline_style(R"(
                width: 150px;
                height: 150px;
                background: white;
                border: 1px solid #ddd;
                border-radius: 5px;
                display: flex;
                align-items: center;
                justify-content: center;
                overflow: hidden;
            )"sv);
        
        // Try to load the image or show filename
        auto label = Dom::text(std::filesystem::path(path).filename().string())
            .with_inline_style("font-size: 10px; text-align: center; word-break: break-all;"sv);
        
        item.add_child(label);
        container.add_child(item);
    }
    
    return container.style(Css::empty());
}

Update on_scroll(RefAny& data, CallbackInfo& info) {
    auto d = InfinityState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    auto scroll_pos = info.get_scroll_position();
    if (!scroll_pos) return Update::DoNothing;
    
    // Calculate which items should be visible based on scroll
    size_t items_per_row = 4;
    size_t item_height = 160; // 150px + 10px gap
    size_t new_start = static_cast<size_t>(scroll_pos->y / item_height) * items_per_row;
    
    if (new_start != d->visible_start) {
        d->visible_start = std::min(new_start, 
            d->file_paths.empty() ? 0 : d->file_paths.size() - 1);
        return Update::RefreshDom;
    }
    
    return Update::DoNothing;
}

std::vector<std::string> scan_image_directory(const std::string& path) {
    std::vector<std::string> files;
    
    try {
        for (const auto& entry : std::filesystem::directory_iterator(path)) {
            if (entry.is_regular_file()) {
                auto ext = entry.path().extension().string();
                // Check for image/svg extensions
                if (ext == ".png" || ext == ".jpg" || ext == ".jpeg" || 
                    ext == ".gif" || ext == ".svg" || ext == ".bmp") {
                    files.push_back(entry.path().string());
                }
            }
        }
    } catch (...) {
        // Directory doesn't exist or can't be read
    }
    
    // If no files found, create dummy entries for demonstration
    if (files.empty()) {
        for (int i = 0; i < 1000; ++i) {
            files.push_back(std::format("image_{:04d}.png", i));
        }
    }
    
    return files;
}

int main() {
    InfinityState state;
    
    // Scan for images in common directories
    state.file_paths = scan_image_directory("~/Pictures");
    if (state.file_paths.empty()) {
        state.file_paths = scan_image_directory(".");
    }
    
    auto data = RefAny::new(std::move(state));
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Infinite Scrolling Gallery"sv);
    window.set_size(LogicalSize(800, 600));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
