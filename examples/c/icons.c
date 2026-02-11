/**
 * Icon System Demo for Azul GUI Framework
 * 
 * This example demonstrates:
 * - Loading a custom icon (favicon.ico) and registering it via IconProviderHandle
 * - Creating icons programmatically with AzDom_createIcon
 * - The icon resolution system (icon name -> visual representation)
 * 
 * Compile with: 
 *   gcc -o icons icons.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

// Helper to create AzString from C string
AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Simple vertical layout using just block elements
    AzDom root = AzDom_createDiv();
    AzDom_setInlineStyle(&root, az_str("padding: 20px; background-color: #fff;"));
    
    // Title
    AzDom title = AzDom_createDiv();
    AzDom title_text = AzDom_createText(az_str("Icon System Demo"));
    AzDom_setInlineStyle(&title_text, az_str("font-size: 24px; font-weight: bold;"));
    AzDom_addChild(&title, title_text);
    AzDom_addChild(&root, title);
    
    // Description  
    AzDom desc = AzDom_createDiv();
    AzDom_setInlineStyle(&desc, az_str("margin-top: 16px;"));
    AzDom desc_text = AzDom_createText(az_str("The favicon icon below is loaded from favicon.ico."));
    AzDom_setInlineStyle(&desc_text, az_str("font-size: 14px; color: #666;"));
    AzDom_addChild(&desc, desc_text);
    AzDom_addChild(&root, desc);
    
    // Simple icon display - just the favicon
    AzDom icon_container = AzDom_createDiv();
    AzDom_setInlineStyle(&icon_container, az_str("margin-top: 20px; padding: 16px; background-color: #e8f4fd;"));
    
    AzDom favicon_icon = AzDom_createIcon(az_str("favicon"));
    AzDom_setInlineStyle(&favicon_icon, az_str("width: 48px; height: 48px;"));
    AzDom_addChild(&icon_container, favicon_icon);
    
    AzDom_addChild(&root, icon_container);
    
    // Material icons row
    AzDom icons_label = AzDom_createDiv();
    AzDom_setInlineStyle(&icons_label, az_str("margin-top: 20px;"));
    AzDom icons_label_text = AzDom_createText(az_str("Material Icons:"));
    AzDom_setInlineStyle(&icons_label_text, az_str("font-size: 14px;"));
    AzDom_addChild(&icons_label, icons_label_text);
    AzDom_addChild(&root, icons_label);
    
    // Icons in a simple container
    AzDom icons_container = AzDom_createDiv();
    AzDom_setInlineStyle(&icons_container, az_str("margin-top: 8px; padding: 16px; background-color: #f0f0f0;"));
    
    AzDom home_icon = AzDom_createIcon(az_str("home"));
    AzDom_setInlineStyle(&home_icon, az_str("font-size: 32px;"));
    AzDom_addChild(&icons_container, home_icon);
    
    AzDom settings_icon = AzDom_createIcon(az_str("settings"));
    AzDom_setInlineStyle(&settings_icon, az_str("font-size: 32px;"));
    AzDom_addChild(&icons_container, settings_icon);
    
    AzDom search_icon = AzDom_createIcon(az_str("search"));
    AzDom_setInlineStyle(&search_icon, az_str("font-size: 32px;"));
    AzDom_addChild(&icons_container, search_icon);
    
    AzDom_addChild(&root, icons_container);
    
    // Convert to StyledDom
    AzCss css = AzCss_empty();
    return AzDom_style(&root, css);
}

int main() {
    printf("Azul Icon System Demo\n");
    printf("=====================\n\n");
    
    // Load favicon.ico from assets folder
    AzFilePath favicon_path = AzFilePath_new(az_str("../assets/images/favicon.ico"));
    printf("Loading favicon from: %s\n", "../assets/images/favicon.ico");
    
    // Read the file
    AzResultU8VecFileError file_result = AzFilePath_readBytes(&favicon_path);
    AzU8Vec* favicon_bytes = NULL;
    if (!AzResultU8VecFileError_matchMutOk(&file_result, &favicon_bytes)) {
        printf("Error: Could not load favicon.ico\n");
        printf("Make sure you run this from the examples/c directory.\n");
        return 1;
    }
    
    printf("Loaded %zu bytes\n", favicon_bytes->len);
    
    // Decode the image
    AzU8VecRef bytes_ref = { .ptr = favicon_bytes->ptr, .len = favicon_bytes->len };
    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(bytes_ref);
    AzRawImage* raw_image = NULL;
    if (!AzResultRawImageDecodeImageError_matchMutOk(&decode_result, &raw_image)) {
        printf("Error: Could not decode favicon.ico\n");
        return 1;
    }
    
    // Create ImageRef from RawImage
    AzOptionImageRef opt_image = AzImageRef_newRawimage(*raw_image);
    AzImageRef* favicon_image_ptr = NULL;
    if (!AzOptionImageRef_matchMutSome(&opt_image, &favicon_image_ptr)) {
        printf("Error: Could not create ImageRef from RawImage\n");
        return 1;
    }
    AzImageRef favicon_image = *favicon_image_ptr;
    printf("Decoded favicon: ready to register\n\n");
    
    // Create app config - call the function to properly initialize icon_provider
    AzAppConfig config = AzAppConfig_create();
    
    // Register the favicon on the config's icon provider
    // This is the correct way: icons registered here will be cloned into each window
    AzIconProviderHandle_registerImageIcon(&config.icon_provider, az_str("app-icons"), az_str("favicon"), favicon_image);
    
    // Create empty RefAny for layout callback
    AzString empty_type = az_str("");
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL, 0, 0);
    
    // Create app with our config
    AzApp app = AzApp_create(empty_data, config);
    
    // Create window
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = az_str("Icon System Demo");
    window.window_state.size.dimensions.width = 550.0f;
    window.window_state.size.dimensions.height = 300.0f;
    
    // Run the app
    AzApp_run(&app, window);
    AzApp_delete(&app);
    
    return 0;
}
