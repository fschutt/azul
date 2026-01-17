#include "azul.h"
#include <stdio.h>

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

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
    AzDom_addCssProperty(&label, font_size);

    // Create a proper Button widget instead of a plain div
    AzString button_text = AzString_copyFromBytes("Increase counter", 0, 16);
    AzButton button = AzButton_create(button_text);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button_dom);

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
