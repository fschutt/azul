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

    AzString label_text = AzString_copyFromBytes(buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    
    AzCssProperty font_size = AzCssProperty_fontSize(AzStyleFontSize_px(50.0));
    AzDom_addCssProperty(&label, font_size);

    AzDom button = AzDom_createDiv();
    AzCssProperty flex_grow = AzCssProperty_flexGrow(AzLayoutFlexGrow_new(1.0));
    AzDom_addCssProperty(&button, flex_grow);
    AzString button_text = AzString_copyFromBytes("Increase counter", 0, 16);
    AzDom_addChild(&button, AzDom_createText(button_text));
    
    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    AzRefAny data_clone = AzRefAny_deepCopy(&data);
    AzDom_addCallback(&button, event, data_clone, on_click);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button);

    AzCss css = AzCss_empty();
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
    AzString title = AzString_copyFromBytes("Hello World", 0, 11);
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    return 0;
}
