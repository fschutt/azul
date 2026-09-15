#define _POSIX_C_SOURCE 200809L
#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

typedef struct {
    AzU8Vec vec;
} CStr;

CStr cstr_new(AzString s) {
    CStr result;
    result.vec = AzString_toCStr(&s);
    AzString_delete(&s);
    return result;
}

CStr cstr_new_ref(const AzString* s) {
    CStr result;
    result.vec = AzString_toCStr(s);
    return result;
}

const char* cstr_ptr(const CStr* c) {
    return (const char*)c->vec.ptr;
}

void cstr_free(CStr* c) {
    AzU8Vec_delete(&c->vec);
}

#define MAX_FONTS 64
#define MAX_IMAGES 256

typedef struct {
    char* url;

    AzUrl base_url;
    bool has_base_url;

    AzXml parsed_xml;
    bool has_xml;

    AzFontRef fonts[MAX_FONTS];
    char* font_names[MAX_FONTS];
    size_t font_count;

    AzImageRef images[MAX_IMAGES];
    char* image_urls[MAX_IMAGES];
    size_t image_count;

    AzExternalResourceVec resources;
    bool has_resources;
    size_t next_resource;
    char* current_url;
    size_t fonts_loaded;
    size_t images_loaded;
    size_t stylesheets_found;

    bool is_loading;
    char* status_message;
    char* error_message;
} BrowserData;

void browser_data_init(BrowserData* data, const char* url) {
    memset(data, 0, sizeof(*data));
    data->url = strdup(url);
    data->status_message = strdup("Initializing...");
    data->is_loading = true;
}

void browser_data_set_status(BrowserData* data, const char* status) {
    if (data->status_message) free(data->status_message);
    data->status_message = strdup(status);
}

void browser_data_set_error(BrowserData* data, const char* error) {
    if (data->error_message) free(data->error_message);
    data->error_message = strdup(error);
}

void BrowserData_destructor(void* ptr);
AZ_REFLECT(BrowserData, BrowserData_destructor);

AzUpdate on_page_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_local_file_read(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_font_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_image_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
static void load_next_resource(AzRefAny data_ref);

AzString resolve_url(BrowserData* data, const AzString* url_str) {
    CStr url_cstr = cstr_new_ref(url_str);
    const char* url = cstr_ptr(&url_cstr);

    if (strncmp(url, "http://", 7) == 0 || strncmp(url, "https://", 8) == 0) {
        cstr_free(&url_cstr);
        return AzString_clone(url_str);
    }

    if (data->has_base_url) {
        AzResultUrlUrlParseError result = AzUrl_join(&data->base_url, AzString_clone(url_str));
        cstr_free(&url_cstr);

        if (result.Ok.tag == AzResultUrlUrlParseError_Tag_Ok) {
            AzString resolved = AzString_clone(&result.Ok.payload.href);
            AzUrl_delete(&result.Ok.payload);
            return resolved;
        }
    }

    cstr_free(&url_cstr);
    return AzString_clone(url_str);
}

static void fetch_resource(BrowserData* data, AzRefAny data_ref, const AzString* url_str,
                           const char* tag, AzResumeCallbackType on_result) {
    AzString resolved = resolve_url(data, url_str);
    CStr url_cstr = cstr_new_ref(&resolved);
    printf("[%s] Loading: %s\n", tag, cstr_ptr(&url_cstr));

    if (data->current_url) free(data->current_url);
    data->current_url = strdup(cstr_ptr(&url_cstr));
    cstr_free(&url_cstr);

    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_httpGet(&config, resolved, AzRefAny_clone(&data_ref), on_result);
    AzHttpRequestConfig_delete(&config);
}

static bool take_response(AzRefAny result, const char* tag, AzHttpResponse* out) {
    AzOptionHttpGetResult r = AzHttpGetResult_downcast(result);
    if (r.Some.tag != AzOptionHttpGetResult_Tag_Some) {
        printf("[%s] Failed to fetch: unexpected result payload\n", tag);
        return false;
    }
    AzResultHttpResponseHttpError http_result = r.Some.payload.result;

    if (http_result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("[%s] Failed to fetch\n", tag);
        AzHttpError_delete(&http_result.Err.payload);
        return false;
    }

    AzHttpResponse response = http_result.Ok.payload;

    if (!AzHttpResponse_isSuccess(&response)) {
        printf("[%s] HTTP error: %u\n", tag, response.status_code);
        AzHttpResponse_delete(&response);
        return false;
    }

    if (response.body.len == 0) {
        printf("[%s] Empty response body\n", tag);
        AzHttpResponse_delete(&response);
        return false;
    }

    *out = response;
    return true;
}

static bool store_font(BrowserData* data, AzRefAny result) {
    AzHttpResponse response;
    if (!take_response(result, "FONT", &response)) {
        return false;
    }

    AzU8Vec body = response.body;

    AzU8Vec font_bytes = AzU8Vec_clone(&body);
    AzLoadedFontSource source;
    source.data = font_bytes;
    source.index = 0;
    source.load_outlines = true;

    AzOptionFontRef font_result = AzFontRef_parse(source);

    if (AzOptionFontRef_isNone(&font_result)) {
        printf("[FONT] Failed to parse font\n");
        AzHttpResponse_delete(&response);
        return false;
    }

    data->fonts[data->font_count] = font_result.Some.payload;
    data->font_names[data->font_count] = strdup(data->current_url ? data->current_url : "");
    data->font_count++;

    printf("[FONT] Loaded successfully (%zu bytes)\n", (size_t)body.len);

    AzHttpResponse_delete(&response);
    return true;
}

static bool store_image(BrowserData* data, AzRefAny result) {
    AzHttpResponse response;
    if (!take_response(result, "IMAGE", &response)) {
        return false;
    }

    AzU8Vec body = response.body;

    AzU8Vec image_bytes = AzU8Vec_clone(&body);

    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(
        (AzU8VecRef){ .ptr = image_bytes.ptr, .len = image_bytes.len }
    );

    AzU8Vec_delete(&image_bytes);

    if (decode_result.Err.tag == AzResultRawImageDecodeImageError_Tag_Err) {
        printf("[IMAGE] Failed to decode\n");
        AzHttpResponse_delete(&response);
        return false;
    }

    AzRawImage raw_image = decode_result.Ok.payload;

    AzOptionImageRef image_result = AzImageRef_rawImage(raw_image);

    if (AzOptionImageRef_isNone(&image_result)) {
        printf("[IMAGE] Failed to create ImageRef\n");
        AzHttpResponse_delete(&response);
        return false;
    }

    data->images[data->image_count] = image_result.Some.payload;
    data->image_urls[data->image_count] = strdup(data->current_url ? data->current_url : "");
    data->image_count++;

    printf("[IMAGE] Loaded successfully (%zu bytes, %ux%u)\n",
           (size_t)body.len, (unsigned)raw_image.width, (unsigned)raw_image.height);

    AzHttpResponse_delete(&response);
    return true;
}

AzUpdate on_font_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    if (store_font(d.ptr, result)) d.ptr->fonts_loaded++;
    BrowserDataRefMut_delete(&d);

    load_next_resource(data_ref);
    return AzUpdate_RefreshDom;
}

AzUpdate on_image_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    if (store_image(d.ptr, result)) d.ptr->images_loaded++;
    BrowserDataRefMut_delete(&d);

    load_next_resource(data_ref);
    return AzUpdate_RefreshDom;
}

AzImageRef* find_image_by_url(BrowserData* data, const char* url) {
    for (size_t i = 0; i < data->image_count; i++) {
        if (strcmp(data->image_urls[i], url) == 0) {
            return &data->images[i];
        }
    }
    return NULL;
}

AzFontRef* find_font_by_url(BrowserData* data, const char* url) {
    for (size_t i = 0; i < data->font_count; i++) {
        if (strstr(data->font_names[i], url) != NULL ||
            strstr(url, data->font_names[i]) != NULL) {
            return &data->fonts[i];
        }
    }
    return NULL;
}

static void load_next_resource(AzRefAny data_ref) {
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return;
    }
    BrowserData* data = d.ptr;

    while (data->has_resources && data->next_resource < data->resources.len) {
        size_t i = data->next_resource;
        AzExternalResource* res = &((AzExternalResource*)data->resources.ptr)[i];
        data->next_resource++;

        CStr url_cstr = cstr_new_ref(&res->url);
        CStr elem_cstr = cstr_new_ref(&res->source_element);
        CStr attr_cstr = cstr_new_ref(&res->source_attribute);

        const char* kind_str = "Unknown";
        switch (res->kind) {
            case AzExternalResourceKind_Image: kind_str = "Image"; break;
            case AzExternalResourceKind_Font: kind_str = "Font"; break;
            case AzExternalResourceKind_Stylesheet: kind_str = "Stylesheet"; break;
            case AzExternalResourceKind_Script: kind_str = "Script"; break;
            case AzExternalResourceKind_Icon: kind_str = "Icon"; break;
            case AzExternalResourceKind_Video: kind_str = "Video"; break;
            case AzExternalResourceKind_Audio: kind_str = "Audio"; break;
            default: break;
        }

        printf("  [%zu] %s: %s (<%s %s>)\n",
               i, kind_str, cstr_ptr(&url_cstr),
               cstr_ptr(&elem_cstr), cstr_ptr(&attr_cstr));

        cstr_free(&url_cstr);
        cstr_free(&elem_cstr);
        cstr_free(&attr_cstr);

        bool waiting = false;
        switch (res->kind) {
            case AzExternalResourceKind_Font:
                if (data->font_count >= MAX_FONTS) {
                    printf("[FONT] Max fonts reached, skipping\n");
                } else {
                    fetch_resource(data, data_ref, &res->url, "FONT", on_font_fetched);
                    waiting = true;
                }
                break;
            case AzExternalResourceKind_Image:
            case AzExternalResourceKind_Icon:
                if (data->image_count >= MAX_IMAGES) {
                    printf("[IMAGE] Max images reached, skipping\n");
                } else {
                    fetch_resource(data, data_ref, &res->url, "IMAGE", on_image_fetched);
                    waiting = true;
                }
                break;
            case AzExternalResourceKind_Stylesheet:
                data->stylesheets_found++;
                printf("  [STYLESHEET] External CSS not yet supported\n");
                break;
            default:
                break;
        }

        if (waiting) {
            BrowserDataRefMut_delete(&d);
            return;
        }
    }

    if (data->has_resources) {
        AzExternalResourceVec_delete(&data->resources);
        data->has_resources = false;
    }

    printf("\n[BROWSER] Resources loaded: %zu fonts, %zu images\n", data->fonts_loaded, data->images_loaded);

    char status[128];
    snprintf(status, sizeof(status), "Loaded: %zu fonts, %zu images", data->fonts_loaded, data->images_loaded);
    browser_data_set_status(data, status);

    data->is_loading = false;
    BrowserDataRefMut_delete(&d);
}

bool is_local_file(const char* path) {
    if (strncmp(path, "http://", 7) == 0 || strncmp(path, "https://", 8) == 0) {
        return false;
    }
    return true;
}

static void load_local_file(BrowserData* data, AzRefAny data_ref) {
    browser_data_set_status(data, "Loading local file...");
    printf("\n[BROWSER] Loading local file: %s\n", data->url);

    AzFilePath file_path = { .inner = az_str(data->url) };
    AzFilePath_readString(&file_path, AzRefAny_clone(&data_ref), on_local_file_read);
    AzFilePath_delete(&file_path);
}

AzUpdate on_local_file_read(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    BrowserData* data = d.ptr;

    AzOptionFileReadStringResult r = AzFileReadStringResult_downcast(result);
    if (r.Some.tag != AzOptionFileReadStringResult_Tag_Some) {
        printf("[BROWSER] Failed to read file: %s\n", data->url);
        browser_data_set_error(data, "Failed to read file");
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    AzResultStringFileError read_result = r.Some.payload.result;

    if (read_result.Err.tag == AzResultStringFileError_Tag_Err) {
        printf("[BROWSER] Failed to read file: %s\n", data->url);
        browser_data_set_error(data, "Failed to read file");
        AzFileError_delete(&read_result.Err.payload);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    AzString html = read_result.Ok.payload;

    CStr html_cstr = cstr_new_ref(&html);
    printf("[BROWSER] File loaded (%zu bytes)\n", strlen(cstr_ptr(&html_cstr)));
    browser_data_set_status(data, "Parsing XHTML...");

    printf("[BROWSER] XHTML preview (first 200 chars):\n%.200s\n", cstr_ptr(&html_cstr));
    cstr_free(&html_cstr);

    AzResultXmlXmlError xml_result = AzXml_fromStr(html);

    printf("[BROWSER] XML parse result tag: %d (Ok=%d, Err=%d)\n",
           xml_result.Ok.tag, AzResultXmlXmlError_Tag_Ok, AzResultXmlXmlError_Tag_Err);

    if (xml_result.Ok.tag == AzResultXmlXmlError_Tag_Err) {
        browser_data_set_error(data, "Failed to parse XHTML");
        AzXmlError_delete(&xml_result.Err.payload);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    data->parsed_xml = xml_result.Ok.payload;
    data->has_xml = true;

    printf("[BROWSER] XHTML parsed successfully\n");
    browser_data_set_status(data, "Ready");

    data->is_loading = false;
    BrowserDataRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_window_created(AzRefAny data_ref, AzCallbackInfo info) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    BrowserData* data = d.ptr;

    if (is_local_file(data->url)) {
        load_local_file(data, data_ref);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    browser_data_set_status(data, "Fetching page...");
    printf("\n[BROWSER] Fetching: %s\n", data->url);

    AzResultUrlUrlParseError url_result = AzUrl_parse(az_str(data->url));
    if (url_result.Ok.tag == AzResultUrlUrlParseError_Tag_Ok) {
        data->base_url = url_result.Ok.payload;
        data->has_base_url = true;
    } else {
        browser_data_set_error(data, "Invalid URL");
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_httpGet(&config, az_str(data->url), AzRefAny_clone(&data_ref), on_page_fetched);
    AzHttpRequestConfig_delete(&config);

    BrowserDataRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_page_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    BrowserData* data = d.ptr;

    AzOptionHttpGetResult r = AzHttpGetResult_downcast(result);
    if (r.Some.tag != AzOptionHttpGetResult_Tag_Some) {
        browser_data_set_error(data, "HTTP error: unexpected result payload");
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }
    AzResultHttpResponseHttpError http_result = r.Some.payload.result;

    if (http_result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        AzString err_dbg = AzHttpError_toDbgString(&http_result.Err.payload);
        CStr err_cstr = cstr_new_ref(&err_dbg);
        printf("[BROWSER] HTTP error: %s\n", cstr_ptr(&err_cstr));
        cstr_free(&err_cstr);

        char err_buf[256];
        snprintf(err_buf, sizeof(err_buf), "HTTP error: see console for details");
        browser_data_set_error(data, err_buf);
        AzString_delete(&err_dbg);
        AzHttpError_delete(&http_result.Err.payload);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    AzHttpResponse response = http_result.Ok.payload;

    printf("[BROWSER] Response status: %u, body len: %zu\n", response.status_code, (size_t)response.body.len);

    if (!AzHttpResponse_isSuccess(&response)) {
        char err[64];
        snprintf(err, sizeof(err), "HTTP Error: %u", response.status_code);
        browser_data_set_error(data, err);
        AzHttpResponse_delete(&response);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    printf("[BROWSER] Page fetched (%zu bytes)\n", (size_t)response.body.len);
    browser_data_set_status(data, "Parsing HTML...");

    AzOptionString body_str = AzHttpResponse_bodyAsString(&response);
    printf("[BROWSER] body_str tag: %d (None=%d, Some=%d)\n",
           body_str.None.tag, AzOptionString_Tag_None, AzOptionString_Tag_Some);

    if (AzOptionString_isNone(&body_str)) {
        browser_data_set_error(data, "Empty response body");
        AzHttpResponse_delete(&response);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    AzString html = body_str.Some.payload;

    CStr html_cstr = cstr_new_ref(&html);
    printf("[BROWSER] HTML preview (first 200 chars):\n%.200s\n", cstr_ptr(&html_cstr));
    cstr_free(&html_cstr);

    AzResultXmlXmlError xml_result = AzXml_fromStr(html);

    printf("[BROWSER] XML parse result tag: %d (Ok=%d, Err=%d)\n",
           xml_result.Ok.tag, AzResultXmlXmlError_Tag_Ok, AzResultXmlXmlError_Tag_Err);

    if (xml_result.Ok.tag == AzResultXmlXmlError_Tag_Err) {
        browser_data_set_error(data, "Failed to parse HTML");
        AzXmlError_delete(&xml_result.Err.payload);
        AzHttpResponse_delete(&response);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    data->parsed_xml = xml_result.Ok.payload;
    data->has_xml = true;

    printf("[BROWSER] HTML parsed successfully\n");
    browser_data_set_status(data, "Scanning for resources...");

    AzExternalResourceVec resources = AzXml_scanExternalResources(&data->parsed_xml);

    printf("[BROWSER] Found %zu external resources\n", (size_t)resources.len);

    AzHttpResponse_delete(&response);

    browser_data_set_status(data, "Loading resources...");

    data->resources = resources;
    data->has_resources = true;
    data->next_resource = 0;
    data->fonts_loaded = 0;
    data->images_loaded = 0;
    data->stylesheets_found = 0;
    BrowserDataRefMut_delete(&d);

    load_next_resource(data_ref);
    return AzUpdate_RefreshDom;
}

AzDom layout(AzRefAny data_ref, AzLayoutCallbackInfo info) {
    (void)info;
    BrowserDataRef d = BrowserDataRef_create(&data_ref);
    if (!BrowserData_downcastRef(&data_ref, &d)) {
        return AzDom_createBody();
    }
    const BrowserData* data = d.ptr;

    if (data->error_message != NULL) {
        AzDom body = AzDom_createBody();
        AzDom_addChild(&body, AzDom_createPWithText(az_str(data->error_message)));
        BrowserDataRef_delete(&d);
        return body;
    }

    if (!data->has_xml || data->is_loading) {
        AzDom body = AzDom_createBody();
        AzDom_addChild(&body, AzDom_createPWithText(az_str(data->status_message ? data->status_message : "Loading...")));
        BrowserDataRef_delete(&d);
        return body;
    }

    AzXml xml_clone = AzXml_clone(&data->parsed_xml);
    AzDom dom = AzDom_createFromParsedXml(xml_clone);
    BrowserDataRef_delete(&d);
    return dom;
}

void BrowserData_destructor(void* ptr) {
    BrowserData* data = (BrowserData*)ptr;

    if (data->url) free(data->url);
    if (data->status_message) free(data->status_message);
    if (data->error_message) free(data->error_message);
    if (data->current_url) free(data->current_url);

    if (data->has_base_url) {
        AzUrl_delete(&data->base_url);
    }

    if (data->has_xml) {
        AzXml_delete(&data->parsed_xml);
    }

    if (data->has_resources) {
        AzExternalResourceVec_delete(&data->resources);
    }

    for (size_t i = 0; i < data->font_count; i++) {
        AzFontRef_delete(&data->fonts[i]);
        if (data->font_names[i]) free(data->font_names[i]);
    }

    for (size_t i = 0; i < data->image_count; i++) {
        AzImageRef_delete(&data->images[i]);
        if (data->image_urls[i]) free(data->image_urls[i]);
    }
}

int main(int argc, char** argv) {
    const char* url = "https://news.ycombinator.com";

    if (argc > 1) {
        url = argv[1];
    }

    printf("=== Azul Simple Browser ===\n");
    printf("URL/File: %s\n\n", url);
    printf("Usage: ./browser <url or file.xht>\n");
    printf("  URL:  ./browser https://news.ycombinator.com\n");
    printf("  File: ./browser test.xht\n\n");
    printf("Note: This is a static browser demo without JavaScript support.\n");
    printf("It demonstrates fetching HTML, parsing it, downloading resources,\n");
    printf("and using FontRef/ImageRef for rendering.\n\n");

    BrowserData model;
    browser_data_init(&model, url);
    AzRefAny data = BrowserData_upcast(model);

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.create_callback = AzOptionCallback_some(AzCallback_create(on_window_created));

    char title[256];
    snprintf(title, sizeof(title), "Azul Browser - %s", url);
    window.window_state.title = az_str(title);

    window.window_state.size.dimensions.width = 1024;
    window.window_state.size.dimensions.height = 768;

    AzApp_run(&app, window);
    AzApp_delete(&app);

    return 0;
}
