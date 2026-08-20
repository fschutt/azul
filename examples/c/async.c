#include "azul.h"
#include <stdio.h>
#include <string.h>

typedef struct {
    float progress;
    bool is_running;
} AppState;

void AppState_destructor(void* s) { }
AZ_REFLECT(AppState, AppState_destructor);

typedef struct {
    float new_progress;
} ProgressUpdate;

void ProgressUpdate_destructor(void* p) { }
AZ_REFLECT(ProgressUpdate, ProgressUpdate_destructor);

typedef struct {
    uint8_t _unused;  // C requires at least one field
} ThreadInitData;

void ThreadInitData_destructor(void* p) { }
AZ_REFLECT(ThreadInitData, ThreadInitData_destructor);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info);
void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv);
AzUpdate writeback_callback(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info);

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    
    AppStateRef state = AppStateRef_create(&data);
    if (!AppState_downcastRef(&data, &state)) {
        return AzDom_createBody();
    }
    
    AzDom body = AzDom_createBody();
    AzString body_style = str("padding: 40px; font-family: sans-serif; align-items: center;");
    AzDom_setCss(&body, body_style);
    
    AzString title_text = str("Background Thread Progress Demo");
    AzDom title = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(title_text);
    AzString title_style = str("font-size: 24px; margin-bottom: 30px;");
    AzDom_setCss(&title, title_style);
    AzDom_addChild(&body, title);
    
    AzDom progress = AzProgressBar_dom(AzProgressBar_create(state.ptr->progress));
    AzString progress_style = str("width: 300px; margin-bottom: 20px;");
    AzDom_setCss(&progress, progress_style);
    AzDom_addChild(&body, progress);
    
    char progress_buf[32];
    snprintf(progress_buf, sizeof(progress_buf), "Progress: %.0f%%", state.ptr->progress);
    AzString progress_label_text = str(progress_buf);
    AzDom progress_label = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(progress_label_text);
    AzString progress_label_style = str("margin-bottom: 20px;");
    AzDom_setCss(&progress_label, progress_label_style);
    AzDom_addChild(&body, progress_label);
    
    if (!state.ptr->is_running) {
        AzString button_text = str("Start");
        AzDom button = AzButton_dom(AzButton_create(button_text));
        AzString button_style = str("padding: 10px 30px;");
        AzDom_setCss(&button, button_style);
        AzEventFilter click_event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
        AzDom_addCallback(&button, click_event, AzRefAny_clone(&data), on_start_clicked);
        AzDom_addChild(&body, button);
    } else {
        AzString running_text = str("Processing...");
        AzDom running = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(running_text);
        AzString running_style = str("color: #666;");
        AzDom_setCss(&running, running_style);
        AzDom_addChild(&body, running);
    }
    
    AppStateRef_delete(&state);
    return body;
}

AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info) {
    
    AppStateRefMut state = AppStateRefMut_create(&data);
    if (!AppState_downcastMut(&data, &state)) {
        return AzUpdate_DoNothing;
    }
    
    state.ptr->progress = 0.0f;
    state.ptr->is_running = true;
    AppStateRefMut_delete(&state);
    
    ThreadInitData init_data = { ._unused = 0 };
    AzRefAny thread_init = ThreadInitData_upcast(init_data);

    AzRefAny writeback = AzRefAny_clone(&data);
    AzThread thread = AzThread_create(
        thread_init,
        writeback,
        background_thread_fn
    );
    
    AzThreadId thread_id = AzThreadId_unique();
    AzCallbackInfo_addThread(&info, thread_id, thread);
    
    return AzUpdate_RefreshDom;
}

void background_thread_fn(
    AzRefAny initial_data, 
    AzThreadSender sender, 
    AzThreadReceiver recv
) {
    
    for (int i = 0; i <= 100; i++) {
        
        AzOptionThreadSendMsg msg = AzThreadReceiver_recv(&recv);
        if (msg.None.tag == AzOptionThreadSendMsg_Tag_Some) {
            if (msg.Some.payload.TerminateThread.tag == AzThreadSendMsg_Tag_TerminateThread) {
                return;
            }
        }
        
        ProgressUpdate update = { .new_progress = (float)i };
        AzRefAny update_data = ProgressUpdate_upcast(update);
        
        AzWriteBackCallback wb_callback = {
            .cb = writeback_callback,
            .ctx = AzOptionRefAny_none()
        };
        AzThreadWriteBackMsg wb_msg = {
            .refany = update_data,
            .callback = wb_callback
        };
        AzThreadReceiveMsg thread_msg = AzThreadReceiveMsg_writeBack(wb_msg);
        
        AzThreadSender_send(&sender, thread_msg);
        
        AzThread_sleepMs(50);
    }
}

AzUpdate writeback_callback(
    AzRefAny app_data, 
    AzRefAny incoming_data, 
    AzCallbackInfo info
) {

    AppStateRefMut state = AppStateRefMut_create(&app_data);
    if (!AppState_downcastMut(&app_data, &state)) {
        return AzUpdate_DoNothing;
    }
    
    ProgressUpdateRef update = ProgressUpdateRef_create(&incoming_data);
    if (!ProgressUpdate_downcastRef(&incoming_data, &update)) {
        AppStateRefMut_delete(&state);
        return AzUpdate_DoNothing;
    }
    
    state.ptr->progress = update.ptr->new_progress;
    
    if (state.ptr->progress >= 100.0f) {
        state.ptr->is_running = false;
    }
    
    ProgressUpdateRef_delete(&update);
    AppStateRefMut_delete(&state);
    
    return AzUpdate_RefreshDom;
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    
    AppState initial_state = {
        .progress = 0.0f,
        .is_running = false
    };
    
    AzRefAny data = AppState_upcast(initial_state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString window_title = str("Async Progress Demo");
    window.window_state.title = window_title;
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
