// g++ -std=c++23 -o async async.cpp -lazul

#include <azul.hpp>
#include <format>
#include <vector>
#include <string>

using namespace azul;
using namespace std::string_view_literals;

enum class ConnectionStage {
    NotConnected,
    Connecting,
    Connected,
    LoadingData,
    DataLoaded,
    Error
};

struct AsyncState {
    ConnectionStage stage = ConnectionStage::NotConnected;
    std::string database_url = "postgres://localhost:5432/mydb";
    std::string error_message;
    std::vector<std::string> loaded_data;
    float progress = 0.0f;
    ThreadId background_thread_id;
};

Update start_connection(RefAny& data, CallbackInfo& info);
Update cancel_connection(RefAny& data, CallbackInfo& info);
Update reset_connection(RefAny& data, CallbackInfo& info);
Update on_timer_tick(RefAny& data, TimerCallbackInfo& info);
ThreadReturn background_task(RefAny& data, ThreadCallbackInfo& info);
Update on_thread_finished(RefAny& data, ThreadCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto d = AsyncState::downcast_ref(data);
    if (!d) return StyledDom::default();
    
    auto title = Dom::text("Async Database Connection Demo"sv)
        .with_inline_style("font-size: 24px; margin-bottom: 20px; color: #333;"sv);
    
    Dom content;
    
    switch (d->stage) {
        case ConnectionStage::NotConnected: {
            auto label = Dom::text("Database URL:"sv)
                .with_inline_style("margin-bottom: 5px;"sv);
            
            auto input = TextInput::new()
                .with_text(d->database_url)
                .dom()
                .with_inline_style("margin-bottom: 15px; width: 100%;"sv);
            
            auto button = Dom::div()
                .with_inline_style("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;"sv)
                .with_child(Dom::text("Connect"sv))
                .with_callback(On::MouseUp, data.clone(), start_connection);
            
            content = Dom::div()
                .with_child(label)
                .with_child(input)
                .with_child(button);
            break;
        }
        
        case ConnectionStage::Connecting: {
            auto status = Dom::text("Establishing connection..."sv)
                .with_inline_style("margin-bottom: 10px;"sv);
            
            auto progress_bar = ProgressBar::new(d->progress)
                .dom()
                .with_inline_style("margin-bottom: 15px;"sv);
            
            auto cancel_btn = Dom::div()
                .with_inline_style("padding: 10px 20px; background: #f44336; color: white; cursor: pointer;"sv)
                .with_child(Dom::text("Cancel"sv))
                .with_callback(On::MouseUp, data.clone(), cancel_connection);
            
            content = Dom::div()
                .with_child(status)
                .with_child(progress_bar)
                .with_child(cancel_btn);
            break;
        }
        
        case ConnectionStage::Connected:
        case ConnectionStage::LoadingData: {
            auto status = Dom::text(std::format("Loading data... {:.0f}%"sv, d->progress))
                .with_inline_style("margin-bottom: 10px;"sv);
            
            auto progress_bar = ProgressBar::new(d->progress)
                .dom()
                .with_inline_style("margin-bottom: 15px;"sv);
            
            content = Dom::div()
                .with_child(status)
                .with_child(progress_bar);
            break;
        }
        
        case ConnectionStage::DataLoaded: {
            auto status = Dom::text(std::format("Loaded {} records"sv, d->loaded_data.size()))
                .with_inline_style("margin-bottom: 10px; color: #4CAF50;"sv);
            
            auto data_list = Dom::div()
                .with_inline_style("max-height: 200px; overflow: auto; background: #f5f5f5; padding: 10px;"sv);
            
            for (const auto& item : d->loaded_data) {
                data_list.add_child(Dom::text(item).with_inline_style("margin-bottom: 5px;"sv));
            }
            
            auto reset_btn = Dom::div()
                .with_inline_style("padding: 10px 20px; background: #2196F3; color: white; cursor: pointer; margin-top: 15px;"sv)
                .with_child(Dom::text("Reset"sv))
                .with_callback(On::MouseUp, data.clone(), reset_connection);
            
            content = Dom::div()
                .with_child(status)
                .with_child(data_list)
                .with_child(reset_btn);
            break;
        }
        
        case ConnectionStage::Error: {
            auto error = Dom::text(std::format("Error: {}"sv, d->error_message))
                .with_inline_style("color: #f44336; margin-bottom: 15px;"sv);
            
            auto reset_btn = Dom::div()
                .with_inline_style("padding: 10px 20px; background: #2196F3; color: white; cursor: pointer;"sv)
                .with_child(Dom::text("Try Again"sv))
                .with_callback(On::MouseUp, data.clone(), reset_connection);
            
            content = Dom::div()
                .with_child(error)
                .with_child(reset_btn);
            break;
        }
    }
    
    auto body = Dom::body()
        .with_inline_style("padding: 30px; font-family: sans-serif; max-width: 500px;"sv)
        .with_child(title)
        .with_child(content);
    
    return body.style(Css::empty());
}

Update start_connection(RefAny& data, CallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->stage = ConnectionStage::Connecting;
    d->progress = 0.0f;
    
    // Start a timer to simulate progress
    info.start_timer(Timer::new(data.clone(), on_timer_tick, info.get_system_time_fn())
        .with_interval(Duration::milliseconds(100)));
    
    return Update::RefreshDom;
}

Update on_timer_tick(RefAny& data, TimerCallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->progress += 2.0f;
    
    if (d->progress >= 30.0f && d->stage == ConnectionStage::Connecting) {
        d->stage = ConnectionStage::LoadingData;
    }
    
    if (d->progress >= 100.0f) {
        d->stage = ConnectionStage::DataLoaded;
        d->loaded_data.clear();
        for (int i = 0; i < 10; ++i) {
            d->loaded_data.push_back(std::format("Record {} - Sample data from database", i + 1));
        }
        return Update::RefreshDomAndStopTimer;
    }
    
    return Update::RefreshDom;
}

Update cancel_connection(RefAny& data, CallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->stage = ConnectionStage::NotConnected;
    d->progress = 0.0f;
    
    return Update::RefreshDom;
}

Update reset_connection(RefAny& data, CallbackInfo& info) {
    auto d = AsyncState::downcast_mut(data);
    if (!d) return Update::DoNothing;
    
    d->stage = ConnectionStage::NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    d->error_message.clear();
    
    return Update::RefreshDom;
}

int main() {
    auto data = RefAny::new(AsyncState{});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Async Operations Demo"sv);
    window.set_size(LogicalSize(600, 500));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
