#include <azul.h>
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

    AzString label_text = AzString_copyFromBytes((uint8_t*)buffer, 0, written);
    AzDom label = AzDom_text(label_text);
    AzString label_style = AzString_copyFromBytes((uint8_t*)"font-size:50px;", 0, 15);
    AzDom_setInlineStyle(&label, label_style);

    AzDom button = AzDom_div();
    AzString button_style = AzString_copyFromBytes((uint8_t*)"flex-grow:1;", 0, 12);
    AzDom_setInlineStyle(&button, button_style);
    AzString button_text = AzString_copyFromBytes((uint8_t*)"Increase counter", 0, 16);
    AzDom_addChild(&button, AzDom_text(button_text));
    
    AzEventFilter event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_MouseUp } };
    AzRefAny data_clone = AzRefAny_deepCopy(&data);
    AzDom_addCallback(&button, event, data_clone, on_click);

    AzDom body = AzDom_body();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    return AzStyledDom_new(body, AzCss_empty());
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    AzString title = AzString_copyFromBytes((uint8_t*)"Hello World", 0, 11);
    window.state.title = title;
    window.state.size.dimensions.width = 400.0;
    window.state.size.dimensions.height = 300.0;
    
    AzAppConfig config = { 0 };
    AzApp app = AzApp_new(data, config);
    AzApp_run(&app, window);
    return 0;
}
