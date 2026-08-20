#include "azul20.hpp"
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

    AsyncState() : stage(Stage_NotConnected), database_url("postgres://localhost:5432/mydb"), progress(0.0f) {}
};

struct ProgressUpdate {
    float progress;
};

struct ThreadInit {
    unsigned char unused;
};

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info);
AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info);
void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv);
AzUpdate on_progress(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const AsyncState* d = data_wrapper.downcast_ref<AsyncState>();
    if (!d) return AzDom_createBody();

    Dom title = Dom::create_text_do_not_use_without_block_level_wrapper(String("Async Database Connection"))
        .with_css(String("font-size: 24px; margin-bottom: 20px;"));

    Dom content = Dom::create_div();
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_MouseUp);

    switch (d->stage) {
        case Stage_NotConnected: {
            content = Dom::create_div()
                .with_css(String("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(String("Connect")))
                .with_callback(event, data_wrapper.clone(), start_connection);
            break;
        }
        case Stage_Connecting:
        case Stage_LoadingData: {
            std::ostringstream ss;
            ss << (d->stage == Stage_Connecting ? "Connecting to " : "Loading from ")
               << d->database_url << " - " << static_cast<int>(d->progress) << "%";
            content = Dom::create_div()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(String(ss.str().c_str())));
            break;
        }
        case Stage_DataLoaded: {
            std::ostringstream ss;
            ss << "Loaded " << d->loaded_data.size() << " records";
            content = Dom::create_div()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(String(ss.str().c_str())))
                .with_child(Dom::create_div()
                    .with_css(String("padding: 10px; background: #2196F3; color: white; cursor: pointer;"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(String("Reset")))
                    .with_callback(event, data_wrapper.clone(), reset_connection));
            break;
        }
        case Stage_Error:
            content = Dom::create_text_do_not_use_without_block_level_wrapper(String("Error occurred"));
            break;
    }

    Dom body = Dom::create_body()
        .with_css(String("padding: 30px; font-family: sans-serif;"))
        .with_child(std::move(title))
        .with_child(std::move(content));

    return std::move(body);
}

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    AsyncState* d = data_wrapper.downcast_mut<AsyncState>();
    if (!d) return AzUpdate_DoNothing;
    if (d->stage == Stage_Connecting || d->stage == Stage_LoadingData) return AzUpdate_DoNothing;

    d->stage = Stage_Connecting;
    d->progress = 0.0f;
    d->loaded_data.clear();

    ThreadInit init = { 0 };
    CallbackInfo cb_info(info);
    cb_info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::create(init), data_wrapper.clone(), background_thread_fn)
    );

    return AzUpdate_RefreshDom;
}

AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    AsyncState* d = data_wrapper.downcast_mut<AsyncState>();
    if (!d) return AzUpdate_DoNothing;
    d->stage = Stage_NotConnected;
    d->progress = 0.0f;
    d->loaded_data.clear();
    return AzUpdate_RefreshDom;
}

void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv) {
    RefAny init_wrapper(initial_data);
    ThreadSender sender_wrapper(sender);
    ThreadReceiver recv_wrapper(recv);

    for (int i = 0; i <= 100; i += 5) {
        OptionThreadSendMsg msg = recv_wrapper.recv();
        if (msg.isSome() && msg.unwrap().TerminateThread.tag == AzThreadSendMsg_Tag_TerminateThread) {
            return;
        }

        ProgressUpdate update = { static_cast<float>(i) };
        sender_wrapper.send(AzThreadReceiveMsg_writeBack(
            ThreadWriteBackMsg::create(on_progress, RefAny::create(update)).release()
        ));

        (void)Thread::sleep_ms(50);
    }
}

AzUpdate on_progress(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info) {
    RefAny app_wrapper(app_data);
    RefAny incoming_wrapper(incoming_data);

    AsyncState* d = app_wrapper.downcast_mut<AsyncState>();
    if (!d) return AzUpdate_DoNothing;

    const ProgressUpdate* update = incoming_wrapper.downcast_ref<ProgressUpdate>();
    if (!update) return AzUpdate_DoNothing;

    d->progress = update->progress;
    if (d->progress >= 100.0f) {
        d->stage = Stage_DataLoaded;
        d->loaded_data.clear();
        for (int i = 0; i < 42; ++i) {
            d->loaded_data.push_back("record_" + std::to_string(i));
        }
    } else if (d->progress >= 50.0f) {
        d->stage = Stage_LoadingData;
    }

    return AzUpdate_RefreshDom;
}

int main() {
    RefAny data = RefAny::create(AsyncState());

    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));

    return 0;
}
