#include "azul.h"
#include <stdio.h>
#include <string.h>

// Helper macro to avoid -Wpointer-sign warnings
#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }

// Forward declarations for JSON serialization/deserialization
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);

// Use AZ_REFLECT_JSON to enable HTTP GetAppState/SetAppState debugging
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);

// ============================================================================
// JSON Serialization
// ============================================================================

AzJson MyDataModel_toJson(AzRefAny refany) {
    MyDataModelRef ref = MyDataModelRef_create(&refany);
    if (!MyDataModel_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    
    int64_t counter = (int64_t)ref.ptr->counter;
    MyDataModelRef_delete(&ref);
    
    return AzJson_int(counter);
}

AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionI64 counter_opt = AzJson_asInt(&json);
    
    if (counter_opt.None.tag == AzOptionI64_Tag_None) {
        return AzResultRefAnyString_err(AZ_STR("Expected integer"));
    }
    
    MyDataModel model = {
        .counter = (uint32_t)counter_opt.Some.payload
    };
    
    AzRefAny refany = MyDataModel_upcast(model);
    return AzResultRefAnyString_ok(refany);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzString label_text = AzString_copyFromBytes(buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    
    AzCssProperty font_size = AzCssProperty_fontSize(AzStyleFontSize_px(50.0));
    AzCssPropertyWithConditions prop = AzCssPropertyWithConditions_simple(font_size);
    AzDom_addCssProperty(&label, prop);

    // Create a proper Button widget instead of a plain div
    AzString button_text = AzString_copyFromBytes("Increase counter", 0, 16);
    AzButton button = AzButton_create(button_text);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button_dom);

    // Use empty CSS - rely on native styling
    AzCss css = AzCss_empty();
    return AzDom_style(&body, css);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    printf("[C CALLBACK] on_click called!\n");
    fflush(stdout);
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        printf("[C CALLBACK] downcast failed!\n");
        fflush(stdout);
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    printf("[C CALLBACK] counter incremented to %d\n", d.ptr->counter);
    fflush(stdout);
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);
    
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzString title = AzString_copyFromBytes("Hello World", 0, 11);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
