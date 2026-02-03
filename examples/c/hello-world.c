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
static inline AzStyleTextColor make_text_color(AzColorU color) {
    AzStyleTextColor result = { .inner = color };
    return result;
}

static inline AzStyleBorderBottomColor make_border_bottom_color(AzColorU color) {
    AzStyleBorderBottomColor result = { .inner = color };
    return result;
}

static inline AzStyleBorderBottomStyle make_border_bottom_style_solid(void) {
    AzStyleBorderBottomStyle result = { .inner = AzBorderStyle_solid() };
    return result;
}

static inline AzLayoutBorderBottomWidth make_border_bottom_width_px(float px) {
    AzLayoutBorderBottomWidth result = { .inner = AzPixelValue_px(px) };
    return result;
}

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

    // Get the system style for native look & feel
    AzSystemStyle system_style = AzSystemStyle_create();
    
    // Determine if dark mode by checking the theme
    bool is_dark = (system_style.theme == AzTheme_Dark);
    
    // Get colors from system style
    AzColorU text_color = is_dark 
        ? AzColorU_rgb(255, 255, 255)  // White text for dark mode
        : AzColorU_rgb(0, 0, 0);       // Black text for light mode
    
    AzColorU window_bg = is_dark
        ? AzColorU_rgb(30, 30, 30)     // Dark background
        : AzColorU_rgb(255, 255, 255); // Light background
    
    // =========================================================================
    // Custom Titlebar - drawn by us since we use NoTitle decorations
    // =========================================================================
    
    // Get title bar colors based on theme
    AzColorU titlebar_bg = is_dark 
        ? AzColorU_rgb(45, 45, 45)     // Dark titlebar
        : AzColorU_rgb(240, 240, 240); // Light titlebar
    
    // Title text - wrapped in a div so it can participate in flex layout
    AzDom title_text = AzDom_createText(AZ_STR("Hello World - Custom Title"));
    AzDom title_container = AzDom_createDiv();
    AzDom_addChild(&title_container, title_text);
    
    // Style the title container (the div is the flex item)
    AzDom_addCssProperty(&title_container, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexGrow(AzLayoutFlexGrow_create(1.0))
    ));
    AzDom_addCssProperty(&title_container, AzCssPropertyWithConditions_simple(
        AzCssProperty_textAlign(AzStyleTextAlign_Center)
    ));
    AzDom_addCssProperty(&title_container, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(13.0))
    ));
    AzDom_addCssProperty(&title_container, AzCssPropertyWithConditions_simple(
        AzCssProperty_textColor(make_text_color(text_color))
    ));
    
    // Create titlebar container
    AzDom titlebar = AzDom_createDiv();
    AzDom_addChild(&titlebar, title_container);
    
    // Style the titlebar
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_display(AzLayoutDisplay_Flex)
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexDirection(AzLayoutFlexDirection_Row)
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_alignItems(AzLayoutAlignItems_Center)
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_justifyContent(AzLayoutJustifyContent_Center)
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_height(AzLayoutHeight_px(AzPixelValue_px(32.0)))
    ));
    // Use align-self: stretch instead of width: 100% for proper flex behavior
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_alignSelf(AzLayoutAlignSelf_Stretch)
    ));
    
    // Titlebar background color
    AzStyleBackgroundContent bg_content = AzStyleBackgroundContent_color(titlebar_bg);
    AzStyleBackgroundContentVec bg_vec = AzStyleBackgroundContentVec_fromItem(bg_content);
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_backgroundContent(bg_vec)
    ));
    
    // Titlebar bottom border
    AzColorU border_color = is_dark 
        ? AzColorU_rgb(60, 60, 60)
        : AzColorU_rgb(200, 200, 200);
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_borderBottomWidth(make_border_bottom_width_px(1.0))
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_borderBottomStyle(make_border_bottom_style_solid())
    ));
    AzDom_addCssProperty(&titlebar, AzCssPropertyWithConditions_simple(
        AzCssProperty_borderBottomColor(make_border_bottom_color(border_color))
    ));

    // =========================================================================
    // Main Content Area
    // =========================================================================
    
    // Counter label
    AzString label_text = AzString_copyFromBytes((const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    
    AzDom_addCssProperty(&label, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(50.0))
    ));
    AzDom_addCssProperty(&label, AzCssPropertyWithConditions_simple(
        AzCssProperty_textColor(make_text_color(text_color))
    ));

    // Create a proper Button widget with Primary style
    AzString button_text = AZ_STR("Increase counter");
    AzButton button = AzButton_create(button_text);
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    // Content container
    AzDom content = AzDom_createDiv();
    AzDom_addChild(&content, label);
    AzDom_addChild(&content, button_dom);
    
    // Style the content
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_display(AzLayoutDisplay_Flex)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexDirection(AzLayoutFlexDirection_Column)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_alignItems(AzLayoutAlignItems_Center)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_justifyContent(AzLayoutJustifyContent_Center)
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexGrow(AzLayoutFlexGrow_create(1.0))
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_paddingTop(make_padding_top_px(20.0))
    ));
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_paddingBottom(make_padding_bottom_px(20.0))
    ));
    
    // Content background
    AzStyleBackgroundContent content_bg_content = AzStyleBackgroundContent_color(window_bg);
    AzStyleBackgroundContentVec content_bg_vec = AzStyleBackgroundContentVec_fromItem(content_bg_content);
    AzDom_addCssProperty(&content, AzCssPropertyWithConditions_simple(
        AzCssProperty_backgroundContent(content_bg_vec)
    ));

    // =========================================================================
    // Body - contains titlebar + content
    // =========================================================================
    
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, titlebar);
    AzDom_addChild(&body, content);
    
    // Style body for flexbox layout
    AzDom_addCssProperty(&body, AzCssPropertyWithConditions_simple(
        AzCssProperty_display(AzLayoutDisplay_Flex)
    ));
    AzDom_addCssProperty(&body, AzCssPropertyWithConditions_simple(
        AzCssProperty_flexDirection(AzLayoutFlexDirection_Column)
    ));

    // Clean up
    AzSystemStyle_delete(&system_style);

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
