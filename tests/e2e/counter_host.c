/* The counter app with JSON reflection (AZ_REFLECT_JSON + toJson/fromJson).
 * The undo/redo e2e (undo-redo.sh) and the Export-Code e2e (test_export_code.sh)
 * drive the app state by field name through the debug server, which needs the
 * serialize/deserialize hooks; the shipped examples/c/hello-world.c does not
 * carry them, so this host does. Keep it the same program otherwise. */
#include "azul.h"
#include <stdio.h>
#include <string.h>

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

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
    AzJsonKeyValue kv = AzJsonKeyValue_create(str("counter"), AzJson_int(counter));
    return AzJson_object(AzJsonKeyValueVec_fromItem(kv));
}

AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionJson field = AzJson_getKey(&json, str("counter"));
    if (field.None.tag == AzOptionJson_Tag_None) {
        return AzResultRefAnyString_err(str("Expected object with 'counter'"));
    }
    AzOptionI64 counter_opt = AzJson_asInt(&field.Some.payload);
    if (counter_opt.None.tag == AzOptionI64_Tag_None) {
        return AzResultRefAnyString_err(str("'counter' is not an integer"));
    }
    MyDataModel model = { .counter = (uint32_t)counter_opt.Some.payload };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
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

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    char buffer[20];
    snprintf(buffer, sizeof(buffer), "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzDom label = AzDom_createPWithText(str(buffer));
    AzDom_setCss(&label, str("font-size: 32px;"));

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
