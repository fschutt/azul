/**
 * Icon System Demo for Azul GUI Framework
 *
 * This example demonstrates:
 * - Loading a custom icon (favicon.ico) and registering it via IconProviderHandle
 * - Creating icons programmatically with AzDom_createIcon
 * - The icon resolution system (icon name -> visual representation)
 *
 * Reading the .ico file is asynchronous: AzFilePath_readBytes only REQUESTS
 * the read and delivers the bytes later, through the event loop, to a resume
 * callback (a browser can only answer it asynchronously). So the favicon can
 * no longer be registered on the AppConfig before the app starts: the
 * window-create callback issues the read, and the resume decodes the image,
 * registers it on a clone of the config's icon provider handle (a shared
 * handle, so the running window sees it) and refreshes the DOM.
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

// App state: where the favicon gets registered once it is read
typedef struct {
    AzIconProviderHandle icons;   // shares the provider the window was created with
    bool favicon_ready;
    bool favicon_failed;
} IconDemo;

void IconDemo_destructor(void* p) {
    IconDemo* d = (IconDemo*)p;
    AzIconProviderHandle_delete(&d->icons);
}
AZ_REFLECT(IconDemo, IconDemo_destructor);

AzUpdate on_favicon_read(AzRefAny data, AzCallbackInfo info, AzRefAny result);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    const char* description = "Loading favicon.ico ...";
    IconDemoRef d = IconDemoRef_create(&data);
    if (IconDemo_downcastRef(&data, &d)) {
        if (d.ptr->favicon_ready) {
            description = "The favicon icon below is loaded from favicon.ico.";
        } else if (d.ptr->favicon_failed) {
            description = "favicon.ico could not be loaded (run from the examples/c directory).";
        }
        IconDemoRef_delete(&d);
    }

    // Simple vertical layout using just block elements
    AzDom root = AzDom_createDiv();
    AzDom_setCss(&root, az_str("padding: 20px; background-color: #fff;"));

    // Title
    AzDom title = AzDom_createDiv();
    AzDom title_text = AzDom_createPWithText(az_str("Icon System Demo"));
    AzDom_setCss(&title_text, az_str("font-size: 24px; font-weight: bold;"));
    AzDom_addChild(&title, title_text);
    AzDom_addChild(&root, title);

    // Description
    AzDom desc = AzDom_createDiv();
    AzDom_setCss(&desc, az_str("margin-top: 16px;"));
    AzDom desc_text = AzDom_createPWithText(az_str(description));
    AzDom_setCss(&desc_text, az_str("font-size: 14px; color: #666;"));
    AzDom_addChild(&desc, desc_text);
    AzDom_addChild(&root, desc);

    // Simple icon display - just the favicon
    AzDom icon_container = AzDom_createDiv();
    AzDom_setCss(&icon_container, az_str("margin-top: 20px; padding: 16px; background-color: #e8f4fd;"));

    AzDom favicon_icon = AzDom_createIcon(az_str("favicon"));
    AzDom_setCss(&favicon_icon, az_str("width: 48px; height: 48px;"));
    AzDom_addChild(&icon_container, favicon_icon);

    AzDom_addChild(&root, icon_container);

    // Material icons row
    AzDom icons_label = AzDom_createDiv();
    AzDom_setCss(&icons_label, az_str("margin-top: 20px;"));
    AzDom icons_label_text = AzDom_createPWithText(az_str("Material Icons:"));
    AzDom_setCss(&icons_label_text, az_str("font-size: 14px;"));
    AzDom_addChild(&icons_label, icons_label_text);
    AzDom_addChild(&root, icons_label);

    // Icons in a simple container
    AzDom icons_container = AzDom_createDiv();
    AzDom_setCss(&icons_container, az_str("margin-top: 8px; padding: 16px; background-color: #f0f0f0;"));

    AzDom home_icon = AzDom_createIcon(az_str("home"));
    AzDom_setCss(&home_icon, az_str("font-size: 32px; margin: 0;"));
    AzDom_addChild(&icons_container, home_icon);

    AzDom settings_icon = AzDom_createIcon(az_str("settings"));
    AzDom_setCss(&settings_icon, az_str("font-size: 32px; margin: 0;"));
    AzDom_addChild(&icons_container, settings_icon);

    AzDom search_icon = AzDom_createIcon(az_str("search"));
    AzDom_setCss(&search_icon, az_str("font-size: 32px; margin: 0;"));
    AzDom_addChild(&icons_container, search_icon);

    AzDom_addChild(&root, icons_container);

    // Apply CSS and return DOM
    return root;
}

// Window-create callback: request the favicon bytes (this used to be a
// blocking read in main)
AzUpdate on_window_created(AzRefAny data, AzCallbackInfo info) {
    (void)info;

    // Load favicon.ico from assets folder
    AzFilePath favicon_path = AzFilePath_create(az_str("../assets/images/favicon.ico"));
    printf("Loading favicon from: %s\n", "../assets/images/favicon.ico");

    // Read the file - the bytes arrive in on_favicon_read
    AzFilePath_readBytes(&favicon_path, AzRefAny_clone(&data), on_favicon_read);
    AzFilePath_delete(&favicon_path);
    return AzUpdate_DoNothing;
}

static AzUpdate favicon_failed(IconDemoRefMut* d) {
    d->ptr->favicon_failed = true;
    IconDemoRefMut_delete(d);
    return AzUpdate_RefreshDom;
}

// Resume of the read: decode the .ico and register it as the "favicon" icon
AzUpdate on_favicon_read(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    (void)info;

    IconDemoRefMut d = IconDemoRefMut_create(&data);
    if (!IconDemo_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }

    AzOptionFileReadBytesResult r = AzFileReadBytesResult_downcast(result);
    if (r.Some.tag != AzOptionFileReadBytesResult_Tag_Some) {
        printf("Error: Could not load favicon.ico (unexpected result payload)\n");
        return favicon_failed(&d);
    }
    AzResultU8VecFileError file_result = r.Some.payload.result;
    AzU8Vec* favicon_bytes = NULL;
    if (!AzResultU8VecFileError_matchMutOk(&file_result, &favicon_bytes)) {
        printf("Error: Could not load favicon.ico\n");
        printf("Make sure you run this from the examples/c directory.\n");
        AzResultU8VecFileError_delete(&file_result);
        return favicon_failed(&d);
    }

    printf("Loaded %zu bytes\n", favicon_bytes->len);

    // Decode the image
    AzU8VecRef bytes_ref = { .ptr = favicon_bytes->ptr, .len = favicon_bytes->len };
    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(bytes_ref);
    AzResultU8VecFileError_delete(&file_result);   // the bytes are decoded, done with them
    AzRawImage* raw_image = NULL;
    if (!AzResultRawImageDecodeImageError_matchMutOk(&decode_result, &raw_image)) {
        printf("Error: Could not decode favicon.ico\n");
        return favicon_failed(&d);
    }

    // Create ImageRef from RawImage
    AzOptionImageRef opt_image = AzImageRef_newRawimage(*raw_image);
    AzImageRef* favicon_image_ptr = NULL;
    if (!AzOptionImageRef_matchMutSome(&opt_image, &favicon_image_ptr)) {
        printf("Error: Could not create ImageRef from RawImage\n");
        return favicon_failed(&d);
    }
    AzImageRef favicon_image = *favicon_image_ptr;
    printf("Decoded favicon: ready to register\n\n");

    // Register the favicon on the icon provider the window shares with the config
    AzIconProviderHandle_registerImageIcon(&d.ptr->icons, az_str("app-icons"), az_str("favicon"), favicon_image);
    d.ptr->favicon_ready = true;
    IconDemoRefMut_delete(&d);

    return AzUpdate_RefreshDom;
}

int main() {
    printf("Azul Icon System Demo\n");
    printf("=====================\n\n");

    // Create app config - call the function to properly initialize icon_provider
    AzAppConfig config = AzAppConfig_create();

    // The favicon is registered on this clone of the config's icon provider
    // once its bytes have been read: icons registered on the config's provider
    // are cloned into each window, and the handle is shared, so the window
    // created from this config sees the late registration too.
    IconDemo state = {
        .icons = AzIconProviderHandle_clone(&config.icon_provider),
        .favicon_ready = false,
        .favicon_failed = false,
    };
    AzRefAny data = IconDemo_upcast(state);

    // Create app with our config
    AzApp app = AzApp_create(data, config);

    // Create window; the create callback requests the favicon bytes
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.create_callback = AzOptionCallback_some(AzCallback_create(on_window_created));
    window.window_state.title = az_str("Icon System Demo");
    window.window_state.size.dimensions.width = 550.0f;
    window.window_state.size.dimensions.height = 300.0f;

    // Run the app
    AzApp_run(&app, window);
    AzApp_delete(&app);

    return 0;
}
