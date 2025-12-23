// g++ -std=c++03 -o async async.cpp -lazul

#include "azul03.hpp"

using namespace azul;

struct AsyncState {
    int connection_status;
};
AZ_REFLECT(AsyncState);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const AsyncState* d = AsyncState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    const char* status_text;
    if (d->connection_status == 0) {
        status_text = "Not connected";
    } else if (d->connection_status == 1) {
        status_text = "Connecting...";
    } else {
        status_text = "Connected!";
    }
    
    Dom label = Dom::create_text(String(status_text));
    label.set_inline_style(String("font-size: 32px;"));
    
    Dom body = Dom::create_body();
    body.set_inline_style(String("padding: 20px; font-family: sans-serif;"));
    body.add_child(label);
    
    return body.style(Css::empty()).release();
}

int main() {
    AsyncState state;
    state.connection_status = 0;
    RefAny data = AsyncState_upcast(state);
    
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"Async Demo", 0, 10);
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
