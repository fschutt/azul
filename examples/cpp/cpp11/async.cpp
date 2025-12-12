// Async Operations - C++11
// g++ -std=c++11 -o async async.cpp -lazul

#include <azul.hpp>
#include <vector>
#include <string>
#include <sstream>

using namespace azul;

enum ConnectionStage {
    Stage_NotConnected,
    Stage_Connecting,
    Stage_LoadingData,
    Stage_DataLoaded,
    Stage_Error
};

struct AsyncState {
    ConnectionStage stage;
    std::string database_url;
    std::vector<std::string> loaded_data;
    float progress;
    
    AsyncState() : stage(Stage_NotConnected), 
                   database_url("postgres://localhost:5432/mydb"),
                   progress(0.0f) {}
};

Update start_connection(RefAny& data, CallbackInfo& info);
Update reset_connection(RefAny& data, CallbackInfo& info);
Update on_timer_tick(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    AsyncState* d = AsyncState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto title = Dom::text("Async Database Connection")
        .with_inline_style("font-size: 24px; margin-bottom: 20px;");
    
    Dom content;
    
    switch (d->stage) {
        case Stage_NotConnected: {
            content = Dom::div()
                .with_inline_style("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;")
                .with_child(Dom::text("Connect"))
                .with_callback(On::MouseUp, data.clone(), start_connection);
            break;
        }
        case Stage_Connecting:
        case Stage_LoadingData: {
            std::ostringstream ss;
            ss << "Progress: " << static_cast<int>(d->progress) << "%";
            content = Dom::div()
                .with_child(Dom::text(ss.str()))
                .with_child(ProgressBar::new_bar(d->progress).dom());
            break;
        }
        case Stage_DataLoaded: {
            std::ostringstream ss;
            ss << "Loaded " << d->loaded_data.size() << " records";
            content = Dom::div()
                .with_child(Dom::text(ss.str()))
                .with_child(Dom::div()
                    .with_inline_style("padding: 10px; background: #2196F3; color: white; cursor: pointer;")
                    .with_child(Dom::text("Reset"))
                    .with_callback(On::MouseUp, data.clone(), reset_connection));
            break;
        }
        case Stage_Error:
            content = Dom::text("Error occurred");
            break;
    }
    
    auto body = Dom::body()
        .with_inline_style("padding: 30px; font-family: sans-serif;")
        .with_child(title)
        .with_child(content);
    
    return body.style(Css::empty());
}

Update start_connection(RefAny& data, CallbackInfo& info) {
    AsyncState* d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->stage = Stage_Connecting;
    d->progress = 0.0f;
    info.start_timer(Timer::new_timer(data.clone(), on_timer_tick, info.get_system_time_fn())
        .with_interval(Duration::milliseconds(100)));
    return Update::RefreshDom;
}

Update on_timer_tick(RefAny& data, TimerCallbackInfo& info) {
    AsyncState* d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->progress += 2.0f;
    if (d->progress >= 100.0f) {
        d->stage = Stage_DataLoaded;
        for (int i = 0; i < 10; ++i) {
            std::ostringstream ss;
            ss << "Record " << (i + 1);
            d->loaded_data.push_back(ss.str());
        }
        return Update::RefreshDomAndStopTimer;
    }
    return Update::RefreshDom;
}

Update reset_connection(RefAny& data, CallbackInfo& info) {
    AsyncState* d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    d->stage = Stage_NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    return Update::RefreshDom;
}

int main() {
    AsyncState state;
    auto data = RefAny::new_ref(state);
    auto window = WindowCreateOptions::new_window(layout);
    window.set_title("Async Operations");
    window.set_size(LogicalSize(600, 400));
    auto app = App::new_app(data, AppConfig::default_config());
    app.run(window);
    return 0;
}
