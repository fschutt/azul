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
    AzDom label = AzDom_createText(label_text);
    AzCssProperty font_size = { .FontSize = { .tag = AzCssProperty_Tag_FontSize, .payload = { .Auto = { .tag = AzStyleFontSize_Tag_Exact, .payload = { .inner = { .metric = AzSizeMetric_Px, .number = 50.0 } } } } } };
    AzDom_addCssProperty(&label, font_size);

    AzDom button = AzDom_createDiv();
    AzCssProperty flex_grow = { .FlexGrow = { .tag = AzCssProperty_Tag_FlexGrow, .payload = { .Auto = { .tag = AzStyleFlexGrow_Tag_Exact, .payload = { .inner = 1.0 } } } } };
    AzDom_addCssProperty(&button, flex_grow);
    AzString button_text = AzString_copyFromBytes((uint8_t*)"Increase counter", 0, 16);
    AzDom_addChild(&button, AzDom_createText(button_text));
    
    AzEventFilter event = { .Hover = { .tag = AzEventFilter_Tag_Hover, .payload = AzHoverEventFilter_MouseUp } };
    AzRefAny data_clone = AzRefAny_deepCopy(&data);
    AzCallback cb = { .cb = on_click };
    AzDom_addCallback(&button, event, data_clone, cb);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    AzCssApiWrapper css = { 0 };
    return AzDom_style(&body, css);
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
    
    AzLayoutCallback layout_cb = { .cb = layout };
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_cb);
    AzString title = AzString_copyFromBytes((uint8_t*)"Hello World", 0, 11);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    return 0;
}
