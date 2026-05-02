// g++ -std=c++03 -o hello-world hello-world.cpp -lazul

#include "azul03.hpp"
#include <cstdio>

using namespace azul;

// Data model: plain old struct - the "single source of truth" for app state.
struct MyDataModel {
    uint32_t counter;
};

// In C++03 the wrapper has no template magic, so the destructor cannot be
// synthesised - supply it explicitly, just like in C.
void MyDataModel_destructor(void* m) { (void)m; }

// AZ_REFLECT in C++03 takes the destructor and emits the C-style reflection
// surface, mirroring the C macro:
//
//   MyDataModel_upcast(MyDataModel)                      -> AzRefAny
//   MyDataModelRef_create(AzRefAny*)                     -> MyDataModelRef
//   MyDataModel_downcastRef(AzRefAny*, MyDataModelRef*)  -> bool
//   MyDataModelRef_delete(MyDataModelRef*)
//   MyDataModelRefMut_create(AzRefAny*)                  -> MyDataModelRefMut
//   MyDataModel_downcastMut(AzRefAny*, MyDataModelRefMut*)-> bool
//   MyDataModelRefMut_delete(MyDataModelRefMut*)
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

// All callbacks use the raw Az* types - no wrapper-side coercion in C++03.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    char buffer[20];
    int written = std::snprintf(buffer, sizeof(buffer), "%u", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzString label_text = AzString_copyFromBytes(
        (const uint8_t*)buffer, 0, (size_t)written);
    AzDom label = AzDom_pWithText(label_text);

    AzButton button = AzButton_create(AzString_fromConstStr("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, button_dom);
    return body;
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    (void)info;

    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model;
    model.counter = 5;
    AzRefAny data = MyDataModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzApp app = AzApp_create(data, AzAppConfig_default());
    AzApp_run(app, window);
    return 0;
}
