// g++ -std=c++11 -o async async.cpp -lazul

#include "azul23.hpp"
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
AZ_REFLECT(AsyncState);

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info);
AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const AsyncState* d = AsyncState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    Dom title = Dom::create_text(String("Async Database Connection"))
        .with_inline_style(String("font-size: 24px; margin-bottom: 20px;"));
    
    Dom content = Dom::create_div();
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    
    switch (d->stage) {
        case Stage_NotConnected: {
            content = Dom::create_div()
                .with_inline_style(String("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;"))
                .with_child(Dom::create_text(String("Connect")))
                .with_callback(event, data_wrapper.clone(), start_connection);
            break;
        }
        case Stage_Connecting:
        case Stage_LoadingData: {
            std::ostringstream ss;
            ss << "Progress: " << static_cast<int>(d->progress) << "%";
            content = Dom::create_div()
                .with_child(Dom::create_text(String(ss.str().c_str())));
            break;
        }
        case Stage_DataLoaded: {
            std::ostringstream ss;
            ss << "Loaded " << d->loaded_data.size() << " records";
            content = Dom::create_div()
                .with_child(Dom::create_text(String(ss.str().c_str())))
                .with_child(Dom::create_div()
                    .with_inline_style(String("padding: 10px; background: #2196F3; color: white; cursor: pointer;"))
                    .with_child(Dom::create_text(String("Reset")))
                    .with_callback(event, data_wrapper.clone(), reset_connection));
            break;
        }
        case Stage_Error:
            content = Dom::create_text(String("Error occurred"));
            break;
    }
    
    Dom body = Dom::create_body()
        .with_inline_style(String("padding: 30px; font-family: sans-serif;"))
        .with_child(std::move(title))
        .with_child(std::move(content));
    
    return body.style(Css::empty()).release();
}

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    AsyncState* d = AsyncState_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->stage = Stage_Connecting;
    d->progress = 0.0f;
    // Timer setup would go here
    return AzUpdate_RefreshDom;
}

AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    AsyncState* d = AsyncState_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->stage = Stage_NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    return AzUpdate_RefreshDom;
}

int main() {
    AsyncState state = { Stage_NotConnected, "postgres://localhost:5432/mydb", {}, 0.0f };
    RefAny data = AsyncState_upcast(state);
    
    LayoutCallback layout_cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(std::move(layout_cb));
    
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    
    return 0;
}
