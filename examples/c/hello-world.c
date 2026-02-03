#include "azul.h"
#include <stdio.h>
#include <string.h>

// Helper macro to avoid -Wpointer-sign warnings
#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }

// Forward declarations for JSON serialization/deserialization
AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);

// Use AZ_REFLECT_JSON to enable HTTP GetAppState/SetAppState debugging
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);

// ============================================================================
// JSON Serialization
// ============================================================================

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
    
    MyDataModel model = {
        .counter = (uint32_t)counter_opt.Some.payload
    };
    
    AzRefAny refany = MyDataModel_upcast(model);
    return AzResultRefAnyString_ok(refany);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Helper functions to create CSS types
static inline AzLayoutPaddingTop make_padding_top_px(float px) {
    AzLayoutPaddingTop result = { .inner = AzPixelValue_px(px) };
    return result;
}

static inline AzLayoutPaddingBottom make_padding_bottom_px(float px) {
    AzLayoutPaddingBottom result = { .inner = AzPixelValue_px(px) };
    return result;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    // =========================================================================
    // Custom Titlebar - using the Titlebar widget
    // =========================================================================
    
    AzTitlebar titlebar = AzTitlebar_create(AZ_STR("Hello World"));
    AzDom titlebar_dom = AzTitlebar_dom(titlebar);

    // =========================================================================
    // Main Content Area
    // =========================================================================
    
    // Counter label
    AzString label_text = AzString_copyFromBytes((const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    
    AzDom_addCssProperty(&label, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(32.0))
    ));

    // Create a proper Button widget with Primary style
    AzString button_text = AZ_STR("Increase counter");
    AzButton button = AzButton_create(button_text);
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    // Content container - simple column layout, no centering
    AzDom content = AzDom_createDiv();
    AzDom_addChild(&content, label);
    AzDom_addChild(&content, button_dom);
    
    // Style the content - simple flex column
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_display(AzLayoutDisplay_Flex)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexDirection(AzLayoutFlexDirection_Column)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexGrow(AzLayoutFlexGrow_create(1.0))
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_paddingTop(make_padding_top_px(10.0))
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_paddingBottom(make_padding_bottom_px(10.0))
    ));

    // =========================================================================
    // Body - contains titlebar + content, transparent background
    // =========================================================================
    
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, titlebar_dom);
    AzDom_addChild(&body, content);
    
    // Style body for flexbox layout
    AzDom_addCssProperty(&body, AzCssPropertyWithConditions_simple(
        AzCssProperty_display(AzLayoutDisplay_Flex)
    ));
    AzDom_addCssProperty(&body, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexDirection(AzLayoutFlexDirection_Column)
    ));

    // Use empty CSS - styling is inline
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
    AzString title = AZ_STR("Hello World");
    window.window_state.title = title;
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    
    // Use NoTitle decorations - system only draws close/min/max buttons
    // We draw our own title in the DOM
    window.window_state.flags.decorations = AzWindowDecorations_NoTitle;
    
    // Use Sidebar material for macOS-style translucent background
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
