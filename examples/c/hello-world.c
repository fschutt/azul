#include "azul.h"
#include <stdio.h>
#include <string.h>

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    char buffer[20];
    snprintf(buffer, sizeof(buffer), "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzDom label = AzDom_createPWithText(str(buffer));
    AzDom_setCss(&label, str("font-size: 32px; margin: 0;"));

    AzButton button = AzButton_create(str("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button_dom);

    return body;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = str("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
