#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct AsyncState {
    float progress;
    int is_running;
};
AZ_REFLECT(AsyncState);

struct ProgressUpdate {
    float new_progress;
};
AZ_REFLECT(ProgressUpdate);

struct ThreadInitData {
    unsigned char unused;
};
AZ_REFLECT(ThreadInitData);

AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info);
void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv);
AzUpdate on_progress(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const AsyncState* d = AsyncState_downcast_ref(data_wrapper);
    if (!d) return AzDom_createBody();

    char progress_buf[32];
    std::snprintf(progress_buf, sizeof(progress_buf), "Progress: %.0f%%", d->progress);

    Dom title = Dom::create_p_with_text(String("Background Thread Progress"));
    title.set_css(String("font-size: 24px; margin-bottom: 20px;"));

    Dom label = Dom::create_p_with_text(String(progress_buf));
    label.set_css(String("font-size: 32px; margin-bottom: 20px;"));

    Dom body = Dom::create_body();
    body.set_css(String("padding: 20px; font-family: sans-serif;"));
    body.add_child(title);
    body.add_child(label);

    if (d->is_running) {
        Dom running = Dom::create_p_with_text(String("Processing..."));
        running.set_css(String("color: #666;"));
        body.add_child(running);
    } else {
        Dom button_text = Dom::create_p_with_text(String("Start"));
        Dom button = Dom::create_div();
        button.set_css(String("padding: 10px 30px; background: #4CAF50; color: white;"));
        button.add_child(button_text);
        button.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), data_wrapper.clone(), on_start_clicked);
        body.add_child(button);
    }

    return body.release();
}

AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    AsyncState* d = AsyncState_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    if (d->is_running) return AzUpdate_DoNothing;

    d->progress = 0.0f;
    d->is_running = 1;

    ThreadInitData init;
    init.unused = 0;

    AzThread thread = AzThread_create(
        ThreadInitData_upcast(init).release(),
        data_wrapper.clone().release(),
        background_thread_fn
    );
    AzCallbackInfo_addThread(&info, AzThreadId_unique(), thread);

    return AzUpdate_RefreshDom;
}

void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv) {
    for (int i = 0; i <= 100; i += 5) {
        AzOptionThreadSendMsg msg = AzThreadReceiver_recv(&recv);
        if (msg.None.tag == AzOptionThreadSendMsg_Tag_Some &&
            msg.Some.payload.TerminateThread.tag == AzThreadSendMsg_Tag_TerminateThread) {
            return;
        }

        ProgressUpdate update;
        update.new_progress = (float)i;

        AzWriteBackCallback wb_callback;
        wb_callback.cb = on_progress;
        wb_callback.ctx = AzOptionRefAny_none();

        AzThreadWriteBackMsg wb_msg = AzThreadWriteBackMsg_create(
            wb_callback,
            ProgressUpdate_upcast(update).release()
        );
        AzThreadSender_send(&sender, AzThreadReceiveMsg_writeBack(wb_msg));

        (void)AzThread_sleepMs(50);
    }
}

AzUpdate on_progress(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info) {
    RefAny app_wrapper(app_data);
    RefAny incoming_wrapper(incoming_data);

    AsyncState* d = AsyncState_downcast_mut(app_wrapper);
    if (!d) return AzUpdate_DoNothing;

    const ProgressUpdate* update = ProgressUpdate_downcast_ref(incoming_wrapper);
    if (!update) return AzUpdate_DoNothing;

    d->progress = update->new_progress;
    if (d->progress >= 100.0f) {
        d->is_running = 0;
    }

    return AzUpdate_RefreshDom;
}

int main() {
    AsyncState state;
    state.progress = 0.0f;
    state.is_running = 0;
    RefAny data = AsyncState_upcast(state);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = az_string_from_literal("Async Demo");

    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
