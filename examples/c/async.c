// cc -o async async.c -lazul

#include <azul.h>
#include <stdio.h>
#include <string.h>

typedef enum {
    Stage_NotConnected,
    Stage_Connecting,
    Stage_LoadingData,
    Stage_DataLoaded,
    Stage_Error
} ConnectionStage;

#define MAX_RECORDS 10

typedef struct {
    ConnectionStage stage;
    char database_url[256];
    char loaded_data[MAX_RECORDS][64];
    size_t record_count;
    float progress;
} AsyncState;

void AsyncState_destructor(void* s) { }
AZ_REFLECT(AsyncState, AsyncState_destructor);

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info);
AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn on_timer_tick(AzRefAny data, AzTimerCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AsyncStateRef d = AsyncStateRef_create(&data);
    if (!AsyncState_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }
    
    AzString title_text = AzString_copyFromBytes((const uint8_t*)"Async Database Connection", 0, 26);
    AzDom title = AzDom_text(title_text);
    AzString title_style = AzString_copyFromBytes((const uint8_t*)"font-size: 24px; margin-bottom: 20px;", 0, 38);
    AzDom_setInlineStyle(&title, title_style);
    
    AzDom content;
    
    switch (d.ptr->stage) {
        case Stage_NotConnected: {
            content = AzDom_div();
            AzString btn_style = AzString_copyFromBytes((const uint8_t*)"padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;", 0, 73);
            AzDom_setInlineStyle(&content, btn_style);
            AzString connect_text = AzString_copyFromBytes((const uint8_t*)"Connect", 0, 7);
            AzDom_addChild(&content, AzDom_text(connect_text));
            AzEventFilter event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_MouseUp } };
            AzDom_addCallback(&content, event, AzRefAny_deepCopy(&data), start_connection);
            break;
        }
        case Stage_Connecting:
        case Stage_LoadingData: {
            char progress_buf[32];
            snprintf(progress_buf, sizeof(progress_buf), "Progress: %d%%", (int)d.ptr->progress);
            
            content = AzDom_div();
            AzString progress_text = AzString_copyFromBytes((const uint8_t*)progress_buf, 0, strlen(progress_buf));
            AzDom_addChild(&content, AzDom_text(progress_text));
            AzDom_addChild(&content, AzProgressBar_dom(AzProgressBar_new(d.ptr->progress)));
            break;
        }
        case Stage_DataLoaded: {
            char status_buf[64];
            snprintf(status_buf, sizeof(status_buf), "Loaded %zu records", d.ptr->record_count);
            
            AzDom reset_btn = AzDom_div();
            AzString reset_style = AzString_copyFromBytes((const uint8_t*)"padding: 10px; background: #2196F3; color: white; cursor: pointer;", 0, 68);
            AzDom_setInlineStyle(&reset_btn, reset_style);
            AzString reset_text = AzString_copyFromBytes((const uint8_t*)"Reset", 0, 5);
            AzDom_addChild(&reset_btn, AzDom_text(reset_text));
            AzEventFilter reset_event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_MouseUp } };
            AzDom_addCallback(&reset_btn, reset_event, AzRefAny_deepCopy(&data), reset_connection);
            
            content = AzDom_div();
            AzString status_text = AzString_copyFromBytes((const uint8_t*)status_buf, 0, strlen(status_buf));
            AzDom_addChild(&content, AzDom_text(status_text));
            AzDom_addChild(&content, reset_btn);
            break;
        }
        case Stage_Error: {
            AzString error_text = AzString_copyFromBytes((const uint8_t*)"Error occurred", 0, 14);
            content = AzDom_text(error_text);
            break;
        }
    }
    
    AsyncStateRef_delete(&d);
    
    AzDom body = AzDom_body();
    AzString body_style = AzString_copyFromBytes((const uint8_t*)"padding: 30px; font-family: sans-serif;", 0, 40);
    AzDom_setInlineStyle(&body, body_style);
    AzDom_addChild(&body, title);
    AzDom_addChild(&body, content);
    
    return AzStyledDom_new(body, AzCss_empty());
}

AzUpdate start_connection(AzRefAny data, AzCallbackInfo info) {
    AsyncStateRefMut d = AsyncStateRefMut_create(&data);
    if (!AsyncState_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    
    d.ptr->stage = Stage_Connecting;
    d.ptr->progress = 0.0f;
    AsyncStateRefMut_delete(&d);
    
    AzDuration interval = { .System = { .tag = AzDuration_Tag_System, .payload = AzSystemTimeDiff_fromMillis(100) } };
    AzTimer timer = AzTimer_new(AzRefAny_deepCopy(&data), on_timer_tick, AzCallbackInfo_getSystemTimeFn(&info));
    timer = AzTimer_withInterval(timer, interval);
    AzTimerId timer_id = { .id = 1 };
    AzCallbackInfo_addTimer(&info, timer_id, timer);
    
    return AzUpdate_RefreshDom;
}

AzTimerCallbackReturn on_timer_tick(AzRefAny data, AzTimerCallbackInfo info) {
    AsyncStateRefMut d = AsyncStateRefMut_create(&data);
    if (!AsyncState_downcastMut(&data, &d)) {
        return (AzTimerCallbackReturn){ .should_update = AzUpdate_DoNothing };
    }
    
    d.ptr->progress += 2.0f;
    
    if (d.ptr->progress >= 100.0f) {
        d.ptr->stage = Stage_DataLoaded;
        d.ptr->record_count = MAX_RECORDS;
        for (int i = 0; i < MAX_RECORDS; i++) {
            snprintf(d.ptr->loaded_data[i], sizeof(d.ptr->loaded_data[i]), "Record %d", i + 1);
        }
        AsyncStateRefMut_delete(&d);
        return (AzTimerCallbackReturn){ .should_update = AzUpdate_RefreshDom };
    }
    
    AsyncStateRefMut_delete(&d);
    return (AzTimerCallbackReturn){ .should_update = AzUpdate_RefreshDom };
}

AzUpdate reset_connection(AzRefAny data, AzCallbackInfo info) {
    AsyncStateRefMut d = AsyncStateRefMut_create(&data);
    if (!AsyncState_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    
    d.ptr->stage = Stage_NotConnected;
    d.ptr->progress = 0.0f;
    d.ptr->record_count = 0;
    AsyncStateRefMut_delete(&d);
    
    return AzUpdate_RefreshDom;
}

int main() {
    AsyncState state;
    memset(&state, 0, sizeof(state));
    state.stage = Stage_NotConnected;
    strncpy(state.database_url, "postgres://localhost:5432/mydb", sizeof(state.database_url));
    state.progress = 0.0f;
    state.record_count = 0;
    
    AzRefAny data = AsyncState_upcast(state);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Async Operations");
    window.state.size.dimensions.width = 600.0;
    window.state.size.dimensions.height = 400.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
