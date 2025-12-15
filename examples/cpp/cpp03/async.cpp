// g++ -std=c++03 -o async async.cpp -lazul

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
};

void AsyncState_init(AsyncState* s) {
    s->stage = Stage_NotConnected;
    s->database_url = "postgres://localhost:5432/mydb";
    s->progress = 0.0f;
}

Update start_connection(RefAny& data, CallbackInfo& info);
Update reset_connection(RefAny& data, CallbackInfo& info);
Update on_timer_tick(RefAny& data, TimerCallbackInfo& info);

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    AsyncState* d = AsyncState_downcast_ref(data);
    if (!d) return StyledDom_default();
    
    Dom title = Dom_text("Async Database Connection");
    Dom_setInlineStyle(title, "font-size: 24px; margin-bottom: 20px;");
    
    Dom content;
    
    switch (d->stage) {
        case Stage_NotConnected: {
            content = Dom_div();
            Dom_setInlineStyle(content, "
                padding: 10px 20px; 
                background: #4CAF50; 
                color: white; 
                cursor: pointer;
            ");
            Dom_addChild(content, Dom_text("Connect"));
            Dom_setCallback(
                content, 
                On_MouseUp, 
                RefAny_clone(data), 
                start_connection
            );
            break;
        }
        case Stage_Connecting:
        case Stage_LoadingData: {
            std::ostringstream ss;
            ss << "Progress: " << static_cast<int>(d->progress) << "%";
            content = Dom_div();
            Dom_addChild(content, Dom_text(ss.str().c_str()));
            Dom_addChild(content, ProgressBar_dom(ProgressBar_new(d->progress)));
            break;
        }
        case Stage_DataLoaded: {
            std::ostringstream ss;
            ss << "Loaded " << d->loaded_data.size() << " records";
            
            Dom reset_btn = Dom_div();
            Dom_setInlineStyle(reset_btn, "
                padding: 10px; 
                background: #2196F3; 
                color: white; 
                cursor: pointer;
            ");
            Dom_addChild(reset_btn, Dom_text("Reset"));
            Dom_setCallback(
                reset_btn, 
                On_MouseUp, 
                RefAny_clone(data), 
                reset_connection
            );
            
            content = Dom_div();
            Dom_addChild(content, Dom_text(ss.str().c_str()));
            Dom_addChild(content, reset_btn);
            break;
        }
        case Stage_Error:
            content = Dom_text("Error occurred");
            break;
    }
    
    Dom body = Dom_body();
    Dom_setInlineStyle(body, "padding: 30px; font-family: sans-serif;");
    Dom_addChild(body, title);
    Dom_addChild(body, content);
    
    return StyledDom_new(body, Css_empty());
}

Update start_connection(RefAny& data, CallbackInfo& info) {
    AsyncState* d = AsyncState_downcast_mut(data);
    if (!d) return Update_DoNothing;
    d->stage = Stage_Connecting;
    d->progress = 0.0f;
    Timer timer = Timer_new(
        RefAny_clone(data), 
        on_timer_tick, 
        CallbackInfo_getSystemTimeFn(info)
    );
    Timer_setInterval(timer, Duration_milliseconds(100));
    CallbackInfo_startTimer(info, timer);
    return Update_RefreshDom;
}

Update on_timer_tick(RefAny& data, TimerCallbackInfo& info) {
    AsyncState* d = AsyncState_downcast_mut(data);
    if (!d) return Update_DoNothing;
    d->progress += 2.0f;
    if (d->progress >= 100.0f) {
        d->stage = Stage_DataLoaded;
        for (int i = 0; i < 10; ++i) {
            std::ostringstream ss;
            ss << "Record " << (i + 1);
            d->loaded_data.push_back(ss.str());
        }
        return Update_RefreshDomAndStopTimer;
    }
    return Update_RefreshDom;
}

Update reset_connection(RefAny& data, CallbackInfo& info) {
    AsyncState* d = AsyncState_downcast_mut(data);
    if (!d) return Update_DoNothing;
    d->stage = Stage_NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    return Update_RefreshDom;
}

int main() {
    AsyncState state;
    AsyncState_init(&state);
    RefAny data = AsyncState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions_new(layout);
    WindowCreateOptions_setTitle(window, "Async Operations");
    WindowCreateOptions_setSize(window, LogicalSize_new(600, 400));
    
    App app = App_new(data, AppConfig_default());
    App_run(app, window);
    return 0;
}
