// g++ -std=c++03 -o hello-world hello-world.cpp -lazul

#include "azul03.hpp"
#include <cstdio>
#include <cstring>

using namespace azul;

// Data model: plain old struct - the "single source of truth" for app state.
struct MyDataModel {
    uint32_t counter;
};

// AZ_REFLECT(structName) emits per-type registration helpers. In C++03 there's
// no template metaprogramming, so this macro is the only path:
//
//   MyDataModel_upcast(MyDataModel)            -> azul::RefAny
//   MyDataModel_downcast_ref(azul::RefAny&)    -> const MyDataModel*
//   MyDataModel_downcast_mut(azul::RefAny&)    -> MyDataModel*
AZ_REFLECT(MyDataModel)

// All callbacks use the raw Az* types - no wrapper-side coercion in C++03.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    azul::RefAny data_wrapper(data);
    const MyDataModel* d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return AzDom_createBody();

    char buffer[20];
    int written = std::snprintf(buffer, sizeof(buffer), "%u", d->counter);

    AzString label_text = AzString_copyFromBytes(
        (const uint8_t*)buffer, 0, (size_t)written);
    AzDom label = AzDom_pWithText(label_text);

    const char* btn_label = "Increase counter";
    AzString btn_label_str = AzString_copyFromBytes((const uint8_t*)btn_label, 0, strlen(btn_label));
    AzButton button = AzButton_create(btn_label_str);
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

    azul::RefAny data_wrapper(data);
    MyDataModel* d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model;
    model.counter = 5;
    azul::RefAny data = MyDataModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzApp app = AzApp_create(data.release(), AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
