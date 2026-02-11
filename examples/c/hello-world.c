#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

// ── Data model ──────────────────────────────────────────────────────────

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }

AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);

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
    MyDataModel model = { .counter = (uint32_t)counter_opt.Some.payload };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
}

// ── Callback ────────────────────────────────────────────────────────────

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// ── Layout ──────────────────────────────────────────────────────────────

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    // Counter label (wrapped in a div to make it block-level)
    AzString label_text = AzString_copyFromBytes((const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    AzDom label_wrapper = AzDom_createDiv();
    AzDom_addCssProperty(&label_wrapper, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(32.0))
    ));
    AzDom_addChild(&label_wrapper, label);

    // Button
    AzButton button = AzButton_create(AZ_STR("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    // Body
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label_wrapper);
    AzDom_addChild(&body, button_dom);

    return AzDom_style(&body, AzCss_empty());
}

// ── Main ────────────────────────────────────────────────────────────────

int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws close/min/max buttons,
    // framework auto-injects a SoftwareTitlebar with drag support.
    window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
