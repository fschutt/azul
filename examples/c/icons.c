#include "azul.h"
#include <stdio.h>
#include <string.h>

AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

typedef struct {
    AzIconProviderHandle icons;
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

    AzDom root = AzDom_createDiv();
    AzDom_setCss(&root, az_str("padding: 20px; background-color: #fff;"));

    AzDom title = AzDom_createDiv();
    AzDom title_text = AzDom_createPWithText(az_str("Icon System Demo"));
    AzDom_setCss(&title_text, az_str("font-size: 24px; font-weight: bold;"));
    AzDom_addChild(&title, title_text);
    AzDom_addChild(&root, title);

    AzDom desc = AzDom_createDiv();
    AzDom_setCss(&desc, az_str("margin-top: 16px;"));
    AzDom desc_text = AzDom_createPWithText(az_str(description));
    AzDom_setCss(&desc_text, az_str("font-size: 14px; color: #666;"));
    AzDom_addChild(&desc, desc_text);
    AzDom_addChild(&root, desc);

    AzDom icon_container = AzDom_createDiv();
    AzDom_setCss(&icon_container, az_str("margin-top: 20px; padding: 16px; background-color: #e8f4fd;"));

    AzDom favicon_icon = AzDom_createIcon(az_str("favicon"));
    AzDom_setCss(&favicon_icon, az_str("width: 48px; height: 48px;"));
    AzDom_addChild(&icon_container, favicon_icon);

    AzDom_addChild(&root, icon_container);

    AzDom icons_label = AzDom_createDiv();
    AzDom_setCss(&icons_label, az_str("margin-top: 20px;"));
    AzDom icons_label_text = AzDom_createPWithText(az_str("Material Icons:"));
    AzDom_setCss(&icons_label_text, az_str("font-size: 14px;"));
    AzDom_addChild(&icons_label, icons_label_text);
    AzDom_addChild(&root, icons_label);

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

    return root;
}

AzUpdate on_window_created(AzRefAny data, AzCallbackInfo info) {
    (void)info;

    AzFilePath favicon_path = AzFilePath_create(az_str("../assets/images/favicon.ico"));
    printf("Loading favicon from: %s\n", "../assets/images/favicon.ico");

    AzFilePath_readBytes(&favicon_path, AzRefAny_clone(&data), on_favicon_read);
    AzFilePath_delete(&favicon_path);
    return AzUpdate_DoNothing;
}

static AzUpdate favicon_failed(IconDemoRefMut* d) {
    d->ptr->favicon_failed = true;
    IconDemoRefMut_delete(d);
    return AzUpdate_RefreshDom;
}

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

    AzU8VecRef bytes_ref = { .ptr = favicon_bytes->ptr, .len = favicon_bytes->len };
    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(bytes_ref);
    AzResultU8VecFileError_delete(&file_result);
    AzRawImage* raw_image = NULL;
    if (!AzResultRawImageDecodeImageError_matchMutOk(&decode_result, &raw_image)) {
        printf("Error: Could not decode favicon.ico\n");
        return favicon_failed(&d);
    }

    AzOptionImageRef opt_image = AzImageRef_createRawimage(*raw_image);
    AzImageRef* favicon_image_ptr = NULL;
    if (!AzOptionImageRef_matchMutSome(&opt_image, &favicon_image_ptr)) {
        printf("Error: Could not create ImageRef from RawImage\n");
        return favicon_failed(&d);
    }
    AzImageRef favicon_image = *favicon_image_ptr;
    printf("Decoded favicon: ready to register\n\n");

    AzIconProviderHandle_registerImageIcon(&d.ptr->icons, az_str("app-icons"), az_str("favicon"), favicon_image);
    d.ptr->favicon_ready = true;
    IconDemoRefMut_delete(&d);

    return AzUpdate_RefreshDom;
}

int main() {
    printf("Azul Icon System Demo\n");
    printf("=====================\n\n");

    AzAppConfig config = AzAppConfig_create();

    IconDemo state = {
        .icons = AzIconProviderHandle_clone(&config.icon_provider),
        .favicon_ready = false,
        .favicon_failed = false,
    };
    AzRefAny data = IconDemo_upcast(state);

    AzApp app = AzApp_create(data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.create_callback = AzOptionCallback_some(AzCallback_create(on_window_created));
    window.window_state.title = az_str("Icon System Demo");
    window.window_state.size.dimensions.width = 550.0f;
    window.window_state.size.dimensions.height = 300.0f;

    AzApp_run(&app, window);
    AzApp_delete(&app);

    return 0;
}
