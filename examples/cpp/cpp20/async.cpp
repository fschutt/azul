// g++ -std=c++20 -o async async.cpp -lazul

#include <azul.hpp>
#include <format>
#include <vector>
#include <string>

using namespace azul;
using namespace std::string_view_literals;

enum class ConnectionStage {
    NotConnected, Connecting, LoadingData, DataLoaded, Error
};

struct AsyncState {
    ConnectionStage stage = ConnectionStage::NotConnected;
    std::string database_url = "postgres://localhost:5432/mydb";
    std::string error_message;
    std::vector<std::string> loaded_data;
    float progress = 0.0f;
};

Update start_connection(RefAny& data, CallbackInfo& info);
Update reset_connection(RefAny& data, CallbackInfo& info);
Update on_timer_tick(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = AsyncState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto title = Dom::create_text("Async Database Connection"sv)
        .with_inline_style("font-size: 24px; margin-bottom: 20px;"sv);
    
    Dom content;
    
    switch (d->stage) {
        case ConnectionStage::NotConnected: {
            auto button = Dom::create_div()
                .with_inline_style("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;"sv)
                .with_child(Dom::create_text("Connect"sv))
                .with_callback(On::MouseUp, data.clone(), start_connection);
            content = button;
            break;
        }
        case ConnectionStage::Connecting:
        case ConnectionStage::LoadingData: {
            auto status = Dom::create_text(std::format("Progress: {:.0f}%"sv, d->progress));
            auto progress_bar = ProgressBar::new(d->progress).dom();
            content = Dom::create_div().with_child(status).with_child(progress_bar);
            break;
        }
        case ConnectionStage::DataLoaded: {
            auto status = Dom::create_text(std::format("Loaded {} records"sv, d->loaded_data.size()));
            auto reset_btn = Dom::create_div()
                .with_inline_style("padding: 10px; background: #2196F3; color: white; cursor: pointer;"sv)
                .with_child(Dom::create_text("Reset"sv))
                .with_callback(On::MouseUp, data.clone(), reset_connection);
            content = Dom::create_div().with_child(status).with_child(reset_btn);
            break;
        }
        case ConnectionStage::Error: {
            content = Dom::create_text(d->error_message);
            break;
        }
    }
    
    auto body = Dom::create_body()
        .with_inline_style("padding: 30px; font-family: sans-serif;"sv)
        .with_child(title)
        .with_child(content);
    
    return body.style(Css::empty());
}

Update start_connection(RefAny& data, CallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->stage = ConnectionStage::Connecting;
    d->progress = 0.0f;
    info.start_timer(Timer::new(data.clone(), on_timer_tick, info.get_system_time_fn())
        .with_interval(Duration::milliseconds(100)));
    return Update::RefreshDom;
}

Update on_timer_tick(RefAny& data, TimerCallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->progress += 2.0f;
    if (d->progress >= 100.0f) {
        d->stage = ConnectionStage::DataLoaded;
        for (int i = 0; i < 10; ++i) {
            d->loaded_data.push_back(std::format("Record {}", i + 1));
        }
        return Update::RefreshDomAndStopTimer;
    }
    return Update::RefreshDom;
}

Update reset_connection(RefAny& data, CallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->stage = ConnectionStage::NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    return Update::RefreshDom;
}

int main() {
    auto data = RefAny::new(AsyncState{});
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Async Operations"sv);
    window.set_size(LogicalSize(600, 400));
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
