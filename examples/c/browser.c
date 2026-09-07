/**
 * Simple Static Web Browser Demo for Azul GUI Framework
 *
 * This example demonstrates:
 * - Fetching a URL via HTTP
 * - Parsing HTML to Azul's XML DOM
 * - Scanning for external resources (images, fonts, stylesheets)
 * - Downloading and registering fonts as FontRefs
 * - Downloading and creating ImageRefs for images
 * - Rendering the final styled DOM
 *
 * NOTE: This is a simple static browser without JavaScript support.
 * It's meant to demonstrate the Azul API capabilities for rendering
 * HTML content like emails, static pages, etc.
 *
 * Fetching is asynchronous: AzHttpRequestConfig_httpGet and
 * AzFilePath_readString only REQUEST the transfer and deliver the result
 * later, through the event loop, to a resume callback (a browser engine can
 * only answer these asynchronously). The page load is therefore a chain of
 * request/resume steps, started by the window-create callback:
 *
 *   fetch page -> on_page_fetched (parse HTML, scan resources)
 *     -> fetch resource -> on_font_fetched / on_image_fetched -> next resource
 *     -> all done: the parsed DOM is rendered
 *
 * A local file goes read -> on_local_file_read -> rendered. While the chain
 * runs, the window shows the status line.
 *
 * Compile with:
 *   gcc -o browser browser.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 *
 * Note: The azul-dll must be compiled with the 'http' feature:
 *   cargo build -p azul-dll --features http,build-dll --release
 *
 * Usage:
 *   ./browser https://example.com
 */

/* strdup() is POSIX, not ISO C99; strict -std=c99 hides it on Linux glibc
 * (implicit-declaration error) without this feature-test macro (before includes). */
#define _POSIX_C_SOURCE 200809L
#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ============================================================================
// Helper Functions
// ============================================================================

// Helper to create AzString from C string
AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// Helper struct for managing null-terminated C strings from AzString
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

// ============================================================================
// Browser Data Structure
// ============================================================================

#define MAX_FONTS 64
#define MAX_IMAGES 256

typedef struct {
    // The URL we're browsing
    char* url;

    // Base URL for resolving relative paths
    AzUrl base_url;
    bool has_base_url;

    // The fetched and parsed HTML
    AzXml parsed_xml;
    bool has_xml;

    // Downloaded fonts (FontRef + name for CSS matching)
    AzFontRef fonts[MAX_FONTS];
    char* font_names[MAX_FONTS];
    size_t font_count;

    // Downloaded images (ImageRef + URL for <img src> matching)
    AzImageRef images[MAX_IMAGES];
    char* image_urls[MAX_IMAGES];
    size_t image_count;

    // External resources of the page, fetched one after the other
    AzExternalResourceVec resources;
    bool has_resources;
    size_t next_resource;      // index of the next resource to look at
    char* current_url;         // resolved URL of the resource in flight
    size_t fonts_loaded;
    size_t images_loaded;
    size_t stylesheets_found;

    // Loading state
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

// The RefAny holds a copy of the struct; the destructor (defined at the
// bottom of the file) frees what the struct owns
void BrowserData_destructor(void* ptr);
AZ_REFLECT(BrowserData, BrowserData_destructor);

// The request/resume chain
AzUpdate on_page_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_local_file_read(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_font_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
AzUpdate on_image_fetched(AzRefAny data_ref, AzCallbackInfo info, AzRefAny result);
static void load_next_resource(AzRefAny data_ref);

// ============================================================================
// URL Resolution
// ============================================================================

// Resolve a potentially relative URL against the base URL
AzString resolve_url(BrowserData* data, const AzString* url_str) {
    CStr url_cstr = cstr_new_ref(url_str);
    const char* url = cstr_ptr(&url_cstr);

    // If it's already absolute, return as-is
    if (strncmp(url, "http://", 7) == 0 || strncmp(url, "https://", 8) == 0) {
        cstr_free(&url_cstr);
        return AzString_clone(url_str);
    }

    // If we have a base URL, join with it
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

// ============================================================================
// Resource Loading
// ============================================================================

// Issue the GET for one resource. The resume (`on_result`) gets the bytes;
// the resolved URL is kept so the resume can name the font / image after it.
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

// Unpack a fetched resource; false (already reported) if there is no usable body
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

// Parse a fetched font and store it as a FontRef
static bool store_font(BrowserData* data, AzRefAny result) {
    AzHttpResponse response;
    if (!take_response(result, "FONT", &response)) {
        return false;
    }

    // Get the response body as bytes
    AzU8Vec body = response.body;

    // Create LoadedFontSource from the bytes
    // Note: We need to copy the bytes since HttpResponse will be deleted
    AzU8Vec font_bytes = AzU8Vec_clone(&body);
    AzLoadedFontSource source;
    source.data = font_bytes;
    source.index = 0;
    source.load_outlines = true;

    // Parse the font
    AzOptionFontRef font_result = AzFontRef_parse(source);

    if (AzOptionFontRef_isNone(&font_result)) {
        printf("[FONT] Failed to parse font\n");
        AzHttpResponse_delete(&response);
        return false;
    }

    // Store the FontRef
    data->fonts[data->font_count] = font_result.Some.payload;
    data->font_names[data->font_count] = strdup(data->current_url ? data->current_url : "");
    data->font_count++;

    printf("[FONT] Loaded successfully (%zu bytes)\n", (size_t)body.len);

    AzHttpResponse_delete(&response);
    return true;
}

// Decode a fetched image and store it as an ImageRef
static bool store_image(BrowserData* data, AzRefAny result) {
    AzHttpResponse response;
    if (!take_response(result, "IMAGE", &response)) {
        return false;
    }

    // Get the response body as bytes
    AzU8Vec body = response.body;

    // Copy bytes for decoding
    AzU8Vec image_bytes = AzU8Vec_clone(&body);

    // Decode the image (auto-detect format)
    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(
        (AzU8VecRef){ .ptr = image_bytes.ptr, .len = image_bytes.len }
    );

    AzU8Vec_delete(&image_bytes);

    if (decode_result.Err.tag == AzResultRawImageDecodeImageError_Tag_Err) {
        printf("[IMAGE] Failed to decode\n");
        // AzDecodeImageError is a simple enum, no delete needed
        AzHttpResponse_delete(&response);
        return false;
    }

    AzRawImage raw_image = decode_result.Ok.payload;

    // Create ImageRef from RawImage
    AzOptionImageRef image_result = AzImageRef_rawImage(raw_image);

    if (AzOptionImageRef_isNone(&image_result)) {
        printf("[IMAGE] Failed to create ImageRef\n");
        AzHttpResponse_delete(&response);
        return false;
    }

    // Store the ImageRef
    data->images[data->image_count] = image_result.Some.payload;
    data->image_urls[data->image_count] = strdup(data->current_url ? data->current_url : "");
    data->image_count++;

    printf("[IMAGE] Loaded successfully (%zu bytes, %ux%u)\n",
           (size_t)body.len, (unsigned)raw_image.width, (unsigned)raw_image.height);

    AzHttpResponse_delete(&response);
    return true;
}

// Resume of a font fetch: store it, continue with the next resource
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

// Resume of an image fetch: store it, continue with the next resource
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

// Find an ImageRef by URL
AzImageRef* find_image_by_url(BrowserData* data, const char* url) {
    for (size_t i = 0; i < data->image_count; i++) {
        if (strcmp(data->image_urls[i], url) == 0) {
            return &data->images[i];
        }
    }
    return NULL;
}

// Find a FontRef by URL (or partial match)
AzFontRef* find_font_by_url(BrowserData* data, const char* url) {
    for (size_t i = 0; i < data->font_count; i++) {
        if (strstr(data->font_names[i], url) != NULL ||
            strstr(url, data->font_names[i]) != NULL) {
            return &data->fonts[i];
        }
    }
    return NULL;
}

// Walk the scanned resources from `next_resource` on: fonts and images are
// fetched one at a time (their resumes call back here), everything else is
// only counted. Once the list is exhausted the page is done loading.
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

        // Load based on resource type
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
                // TODO: Fetch and parse external CSS
                printf("  [STYLESHEET] External CSS not yet supported\n");
                break;
            default:
                // Skip scripts, video, audio for now
                break;
        }

        if (waiting) {
            // The resume of this fetch continues the walk
            BrowserDataRefMut_delete(&d);
            return;
        }
    }

    // Every resource was handled
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

// ============================================================================
// Local File Loading
// ============================================================================

// Check if path is a local file (not a URL)
bool is_local_file(const char* path) {
    // If it starts with http:// or https://, it's a URL
    if (strncmp(path, "http://", 7) == 0 || strncmp(path, "https://", 8) == 0) {
        return false;
    }
    // Otherwise assume it's a local file path
    return true;
}

// Load a local .xht/.xhtml/.html file: the text arrives in on_local_file_read
static void load_local_file(BrowserData* data, AzRefAny data_ref) {
    browser_data_set_status(data, "Loading local file...");
    printf("\n[BROWSER] Loading local file: %s\n", data->url);

    // Read the file using Azul's file API
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

    // Print first 200 chars for debugging
    printf("[BROWSER] XHTML preview (first 200 chars):\n%.200s\n", cstr_ptr(&html_cstr));
    cstr_free(&html_cstr);

    // Parse XHTML to XML
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

// ============================================================================
// Main Page Loading Logic
// ============================================================================

// Window-create callback: start the page load. This used to block in main();
// now it only issues the first request of the chain.
AzUpdate on_window_created(AzRefAny data_ref, AzCallbackInfo info) {
    (void)info;
    BrowserDataRefMut d = BrowserDataRefMut_create(&data_ref);
    if (!BrowserData_downcastMut(&data_ref, &d)) {
        return AzUpdate_DoNothing;
    }
    BrowserData* data = d.ptr;

    // Check if this is a local file
    if (is_local_file(data->url)) {
        load_local_file(data, data_ref);
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    browser_data_set_status(data, "Fetching page...");
    printf("\n[BROWSER] Fetching: %s\n", data->url);

    // Parse base URL
    AzResultUrlUrlParseError url_result = AzUrl_parse(az_str(data->url));
    if (url_result.Ok.tag == AzResultUrlUrlParseError_Tag_Ok) {
        data->base_url = url_result.Ok.payload;
        data->has_base_url = true;
    } else {
        browser_data_set_error(data, "Invalid URL");
        BrowserDataRefMut_delete(&d);
        return AzUpdate_RefreshDom;
    }

    // Fetch the HTML page - the response arrives in on_page_fetched
    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_httpGet(&config, az_str(data->url), AzRefAny_clone(&data_ref), on_page_fetched);
    AzHttpRequestConfig_delete(&config);

    BrowserDataRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Resume of the page fetch: parse the HTML, scan it for resources and start
// fetching them one by one
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

    // Get body as string
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

    // Print first 200 chars of HTML for debugging
    CStr html_cstr = cstr_new_ref(&html);
    printf("[BROWSER] HTML preview (first 200 chars):\n%.200s\n", cstr_ptr(&html_cstr));
    cstr_free(&html_cstr);

    // Parse HTML to XML
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

    // Scan for external resources
    AzExternalResourceVec resources = AzXml_scanExternalResources(&data->parsed_xml);

    printf("[BROWSER] Found %zu external resources\n", (size_t)resources.len);

    AzHttpResponse_delete(&response);

    // Download resources - one request/resume pair per font or image
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

// ============================================================================
// Layout Callback
// ============================================================================

AzDom layout(AzRefAny data_ref, AzLayoutCallbackInfo info) {
    (void)info;
    BrowserDataRef d = BrowserDataRef_create(&data_ref);
    if (!BrowserData_downcastRef(&data_ref, &d)) {
        return AzDom_createBody();
    }
    const BrowserData* data = d.ptr;

    // If still loading or error, show status
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

    // Convert parsed XML to DOM with CSS from <style> tags attached.
    // The framework applies CSS during the cascade pass.
    AzXml xml_clone = AzXml_clone(&data->parsed_xml);
    AzDom dom = AzDom_createFromParsedXml(xml_clone);
    BrowserDataRef_delete(&d);
    return dom;
}

// ============================================================================
// Destructor Callback
// ============================================================================

// Frees what the struct owns; the struct itself lives inside the RefAny
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

// ============================================================================
// Main
// ============================================================================

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

    // Create browser data; the page load starts once the window exists
    BrowserData model;
    browser_data_init(&model, url);
    AzRefAny data = BrowserData_upcast(model);

    // Create app
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.create_callback = AzOptionCallback_some(AzCallback_create(on_window_created));

    char title[256];
    snprintf(title, sizeof(title), "Azul Browser - %s", url);
    window.window_state.title = az_str(title);

    // Set initial window size
    window.window_state.size.dimensions.width = 1024;
    window.window_state.size.dimensions.height = 768;

    AzApp_run(&app, window);
    AzApp_delete(&app);

    return 0;
}
