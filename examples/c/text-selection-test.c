#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { int dummy; } AppData;
void AppData_destructor(void* d) { }

AzJson AppData_toJson(AzRefAny refany);
AzResultRefAnyString AppData_fromJson(AzJson json);
AZ_REFLECT_JSON(AppData, AppData_destructor, AppData_toJson, AppData_fromJson);

AzJson AppData_toJson(AzRefAny refany) { return AzJson_null(); }
AzResultRefAnyString AppData_fromJson(AzJson json) {
    AppData m = { .dummy = 0 };
    return AzResultRefAnyString_ok(AppData_upcast(m));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Use inline CSS for styling
    AzString css_str = AZ_STR(
        "body { font-size: 16px; padding: 20px; }"
        ".editable { border: 1px solid #999; padding: 10px; margin-top: 20px; min-height: 40px; }"
        ".label { margin-bottom: 10px; color: #666; font-size: 14px; }"
    );
    AzCss css = AzCss_fromString(css_str);

    AzDom body = AzDom_createBody();

    // Label
    AzDom label1 = AzDom_createDiv();
    AzDom_addClass(&label1, AZ_STR("label"));
    AzDom_addChild(&label1, AzDom_createText(AZ_STR("Selectable text (click and drag):")));
    AzDom_addChild(&body, label1);

    // Selectable text paragraph
    AzDom p1 = AzDom_createDiv();
    AzDom_addChild(&p1, AzDom_createText(AZ_STR("The quick brown fox jumps over the lazy dog. This text should be selectable by clicking and dragging.")));
    AzDom_addChild(&body, p1);

    // Label
    AzDom label2 = AzDom_createDiv();
    AzDom_addClass(&label2, AZ_STR("label"));
    AzDom_addChild(&label2, AzDom_createText(AZ_STR("Contenteditable div (click and type):")));
    AzDom_addChild(&body, label2);

    // Contenteditable div
    AzDom editable = AzDom_createDiv();
    AzDom_addClass(&editable, AZ_STR("editable"));
    AzDom_setContenteditable(&editable, true);
    AzDom_addChild(&editable, AzDom_createText(AZ_STR("Click here and type to edit this text.")));
    AzDom_addChild(&body, editable);

    return AzDom_style(body, css);
}

int main() {
    AppData model = { .dummy = 0 };
    AzRefAny data = AppData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Text Selection & Editing Test");
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 400.0;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
