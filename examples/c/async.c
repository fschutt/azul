#include "azul.h"
#include <stdio.h>
#include <string.h>

// Application State
typedef struct {
    float progress;      // 0.0 to 100.0
    bool is_running;     // true if background thread is active
} AppState;

void AppState_destructor(void* s) { }
AZ_REFLECT(AppState, AppState_destructor);

// Message sent from background thread to update progress
typedef struct {
    float new_progress;
} ProgressUpdate;

void ProgressUpdate_destructor(void* p) { }
AZ_REFLECT(ProgressUpdate, ProgressUpdate_destructor);

// Empty struct for thread initialization (demonstrates proper pattern)
typedef struct {
    uint8_t _unused;  // C requires at least one field
} ThreadInitData;

void ThreadInitData_destructor(void* p) { }
AZ_REFLECT(ThreadInitData, ThreadInitData_destructor);

// Forward declarations
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info);
void background_thread_fn(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv);
AzUpdate writeback_callback(AzRefAny app_data, AzRefAny incoming_data, AzCallbackInfo info);

// Helper to create AzString from literal (to work around compound literal issue)
static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    
    AppStateRef state = AppStateRef_create(&data);
    if (!AppState_downcastRef(&data, &state)) {
        return AzStyledDom_default();
    }
    
    // Create main container
    AzDom body = AzDom_createBody();
    AzString body_style = str("padding: 40px; font-family: sans-serif; align-items: center;");
    AzDom_setInlineStyle(&body, body_style);
    
    // Title
    AzString title_text = str("Background Thread Progress Demo");
    AzDom title = AzDom_createText(title_text);
    AzString title_style = str("font-size: 24px; margin-bottom: 30px;");
    AzDom_setInlineStyle(&title, title_style);
    AzDom_addChild(&body, title);
    
    // Progress bar
    AzDom progress = AzProgressBar_dom(AzProgressBar_create(state.ptr->progress));
    AzString progress_style = str("width: 300px; margin-bottom: 20px;");
    AzDom_setInlineStyle(&progress, progress_style);
    AzDom_addChild(&body, progress);
    
    // Progress text
    char progress_buf[32];
    snprintf(progress_buf, sizeof(progress_buf), "Progress: %.0f%%", state.ptr->progress);
    AzString progress_label_text = str(progress_buf);
    AzDom progress_label = AzDom_createText(progress_label_text);
    AzString progress_label_style = str("margin-bottom: 20px;");
    AzDom_setInlineStyle(&progress_label, progress_label_style);
    AzDom_addChild(&body, progress_label);
    
    // Start/Reset button
    if (!state.ptr->is_running) {
        AzString button_text = str("Start");
        AzDom button = AzButton_dom(AzButton_create(button_text));
        AzString button_style = str("padding: 10px 30px;");
        AzDom_setInlineStyle(&button, button_style);
        AzEventFilter click_event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
        AzDom_addCallback(&button, click_event, AzRefAny_clone(&data), on_start_clicked);
        AzDom_addChild(&body, button);
    } else {
        AzString running_text = str("Processing...");
        AzDom running = AzDom_createText(running_text);
        AzString running_style = str("color: #666;");
        AzDom_setInlineStyle(&running, running_style);
        AzDom_addChild(&body, running);
    }
    
    AppStateRef_delete(&state);
    return AzDom_style(&body, AzCss_empty());
}

// Start Button Click Handler
AzUpdate on_start_clicked(AzRefAny data, AzCallbackInfo info) {
    
    // Downcast to mutable AppState
    AppStateRefMut state = AppStateRefMut_create(&data);
    if (!AppState_downcastMut(&data, &state)) {
        return AzUpdate_DoNothing;
    }
    
    // Reset progress and mark as running
    state.ptr->progress = 0.0f;
    state.ptr->is_running = true;
    AppStateRefMut_delete(&state);
    
    // Create the background thread
    // - initial_data: data passed to the thread function when it starts
    // - writeback_data: our app state (will be passed to writeback_callback)
    ThreadInitData init_data = { ._unused = 0 };
    AzRefAny thread_init = ThreadInitData_upcast(init_data);

    // data to write received msgs back into
    AzRefAny writeback = AzRefAny_clone(&data);
    AzThread thread = AzThread_create(
        thread_init,
        writeback,
        background_thread_fn
    );
    
    // Add thread to the event loop
    AzThreadId thread_id = AzThreadId_unique();
    AzCallbackInfo_addThread(&info, thread_id, thread);
    
    return AzUpdate_RefreshDom;
}

// Background Thread Function
// This now runs in a separate thread!
void background_thread_fn(
    AzRefAny initial_data, 
    AzThreadSender sender, 
    AzThreadReceiver recv
) {
    
    // Simulate work: count from 0 to 100
    for (int i = 0; i <= 100; i++) {
        
        // Check if we should terminate
        AzOptionThreadSendMsg msg = AzThreadReceiver_recv(&recv);
        if (msg.None.tag == AzOptionThreadSendMsg_Tag_Some) {
            if (msg.Some.payload.TerminateThread.tag == AzThreadSendMsg_Tag_TerminateThread) {
                return;  // Thread was cancelled
            }
        }
        
        // Create progress update message
        ProgressUpdate update = { .new_progress = (float)i };
        AzRefAny update_data = ProgressUpdate_upcast(update);
        
        // Create writeback message with callback
        AzWriteBackCallback wb_callback = {
            .cb = writeback_callback,
            .ctx = AzOptionRefAny_none()
        };
        AzThreadWriteBackMsg wb_msg = {
            .refany = update_data,
            .callback = wb_callback
        };
        AzThreadReceiveMsg thread_msg = AzThreadReceiveMsg_writeBack(wb_msg);
        
        // Send to main thread
        AzThreadSender_send(&sender, thread_msg);
        
        // Simulate work (sleep 50ms)
        // In real code, this would be actual work like file I/O, network, etc.
        AzThread_sleepMs(50);
    }
}

// Writeback Callback (runs on main thread)
// 
// This is called on the main thread when a message arrives FROM the background
// thread and gets "written back" into the main-thread application state.
AzUpdate writeback_callback(
    AzRefAny app_data, 
    AzRefAny incoming_data, 
    AzCallbackInfo info
) {

    // Downcast app_data to our AppState
    AppStateRefMut state = AppStateRefMut_create(&app_data);
    if (!AppState_downcastMut(&app_data, &state)) {
        return AzUpdate_DoNothing;
    }
    
    // Downcast incoming_data to ProgressUpdate (new incoming update)
    ProgressUpdateRef update = ProgressUpdateRef_create(&incoming_data);
    if (!ProgressUpdate_downcastRef(&incoming_data, &update)) {
        AppStateRefMut_delete(&state);
        return AzUpdate_DoNothing;
    }
    
    // Update the progress
    state.ptr->progress = update.ptr->new_progress;
    
    // If we reached 100%, mark as not running
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
    
    // Initialize application state
    AppState initial_state = {
        .progress = 0.0f,
        .is_running = false
    };
    
    // Wrap state in RefAny for thread-safe reference counting
    AzRefAny data = AppState_upcast(initial_state);
    
    // Create window
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString window_title = str("Async Progress Demo");
    window.window_state.title = window_title;
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 300.0;
    
    // Run application
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
