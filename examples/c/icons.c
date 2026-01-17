/**
 * Icon System Demo for Azul GUI Framework
 * 
 * This example demonstrates:
 * - Loading a custom icon (favicon.ico) and registering it via IconProviderHandle
 * - Creating icons programmatically with AzDom_createIcon
 * - The icon resolution system (icon name -> visual representation)
 * 
 * Compile with: 
 *   gcc -o icons icons.c -I. -L../../target/release -lazul_dll -Wl,-rpath,../../target/release
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

// Helper to create AzString from C string
AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    // Main container
    AzDom root = AzDom_createDiv();
    AzDom_setInlineStyle(&root, az_str("padding: 20px; flex-direction: column; gap: 16px;"));
    
    // Title
    AzDom title = AzDom_createText(az_str("Icon System Demo"));
    AzDom_setInlineStyle(&title, az_str("font-size: 24px; font-weight: bold; margin-bottom: 16px;"));
    AzDom_addChild(&root, title);
    
    // Description
    AzDom desc = AzDom_createText(az_str("The favicon icon below is loaded from favicon.ico and registered via IconProviderHandle."));
    AzDom_setInlineStyle(&desc, az_str("font-size: 14px; color: #666; margin-bottom: 20px;"));
    AzDom_addChild(&root, desc);
    
    // Row with the registered favicon icon
    AzDom icon_row = AzDom_createDiv();
    AzDom_setInlineStyle(&icon_row, az_str("flex-direction: row; gap: 24px; align-items: center;"));
    
    // Card for favicon
    AzDom card = AzDom_createDiv();
    AzDom_setInlineStyle(&card, az_str(
        "flex-direction: column; align-items: center; padding: 16px; "
        "background-color: #e8f4fd; border-radius: 8px; min-width: 100px;"
    ));
    
    // Create the icon node - this will be resolved to the registered favicon
    AzDom favicon_icon = AzDom_createIcon(az_str("favicon"));
    AzDom_setInlineStyle(&favicon_icon, az_str("width: 48px; height: 48px; margin-bottom: 8px;"));
    AzDom_addChild(&card, favicon_icon);
    
    AzDom label = AzDom_createText(az_str("favicon"));
    AzDom_setInlineStyle(&label, az_str("font-size: 12px; color: #666;"));
    AzDom_addChild(&card, label);
    
    AzDom_addChild(&icon_row, card);
    
    // Add some placeholder icons that won't resolve (no font pack registered)
    const char* other_icons[] = {"home", "settings", "search"};
    for (int i = 0; i < 3; i++) {
        AzDom other_card = AzDom_createDiv();
        AzDom_setInlineStyle(&other_card, az_str(
            "flex-direction: column; align-items: center; padding: 16px; "
            "background-color: #f0f0f0; border-radius: 8px; min-width: 80px;"
        ));
        
        AzDom other_icon = AzDom_createIcon(az_str(other_icons[i]));
        AzDom_setInlineStyle(&other_icon, az_str("font-size: 32px; margin-bottom: 8px;"));
        AzDom_addChild(&other_card, other_icon);
        
        AzDom other_label = AzDom_createText(az_str(other_icons[i]));
        AzDom_setInlineStyle(&other_label, az_str("font-size: 12px; color: #999;"));
        AzDom_addChild(&other_card, other_label);
        
        AzDom_addChild(&icon_row, other_card);
    }
    
    AzDom_addChild(&root, icon_row);
    
    // Note
    AzDom note = AzDom_createText(az_str(
        "Note: Only 'favicon' is registered. Other icons show as empty placeholders."
    ));
    AzDom_setInlineStyle(&note, az_str("font-size: 12px; color: #999; margin-top: 20px;"));
    AzDom_addChild(&root, note);
    
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
    
    // Debug: print icon_provider pointer
    printf("DEBUG: config.icon_provider.inner = %p\n", config.icon_provider.inner);
    printf("DEBUG: &config.icon_provider = %p\n", (void*)&config.icon_provider);
    
    // Register the favicon on the config's icon provider
    // This is the correct way: icons registered here will be cloned into each window
    printf("DEBUG: About to call registerImageIcon...\n");
    AzIconProviderHandle_registerImageIcon(&config.icon_provider, az_str("app-icons"), az_str("favicon"), favicon_image);
    
    printf("Registered icon: 'favicon' on config.icon_provider\n\n");
    
    // Create empty RefAny for layout callback
    AzString empty_type = az_str("");
    AzRefAny empty_data = AzRefAny_newC((AzGlVoidPtrConst){.ptr = NULL}, 0, 1, 0, empty_type, NULL);
    
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
