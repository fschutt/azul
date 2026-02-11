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
 * Compile with: 
 *   gcc -o browser browser.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 * 
 * Note: The azul-dll must be compiled with the 'http' feature:
 *   cargo build -p azul-dll --features http,build-dll --release
 * 
 * Usage:
 *   ./browser https://example.com
 */

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
    
    // Loading state
    bool is_loading;
    char* status_message;
    char* error_message;
} BrowserData;

BrowserData* browser_data_new(const char* url) {
    BrowserData* data = (BrowserData*)calloc(1, sizeof(BrowserData));
    data->url = strdup(url);
    data->status_message = strdup("Initializing...");
    return data;
}

void browser_data_set_status(BrowserData* data, const char* status) {
    if (data->status_message) free(data->status_message);
    data->status_message = strdup(status);
}

void browser_data_set_error(BrowserData* data, const char* error) {
    if (data->error_message) free(data->error_message);
    data->error_message = strdup(error);
}

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

// Download and parse a font, returning a FontRef
bool load_font(BrowserData* data, const AzString* url_str) {
    if (data->font_count >= MAX_FONTS) {
        printf("[FONT] Max fonts reached, skipping\n");
        return false;
    }
    
    AzString resolved = resolve_url(data, url_str);
    CStr url_cstr = cstr_new_ref(&resolved);
    printf("[FONT] Loading: %s\n", cstr_ptr(&url_cstr));
    
    // Fetch the font file
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(resolved);
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("[FONT] Failed to fetch\n");
        cstr_free(&url_cstr);
        AzHttpError_delete(&result.Err.payload);
        return false;
    }
    
    AzHttpResponse response = result.Ok.payload;
    
    if (!AzHttpResponse_isSuccess(&response)) {
        printf("[FONT] HTTP error: %u\n", response.status_code);
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
    // Get the response body as bytes
    AzU8Vec body = response.body;
    
    if (body.len == 0) {
        printf("[FONT] Empty response body\n");
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
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
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
    // Store the FontRef
    data->fonts[data->font_count] = font_result.Some.payload;
    data->font_names[data->font_count] = strdup(cstr_ptr(&url_cstr));
    data->font_count++;
    
    printf("[FONT] Loaded successfully (%zu bytes)\n", (size_t)body.len);
    
    cstr_free(&url_cstr);
    AzHttpResponse_delete(&response);
    return true;
}

// Download and decode an image, returning an ImageRef
bool load_image(BrowserData* data, const AzString* url_str) {
    if (data->image_count >= MAX_IMAGES) {
        printf("[IMAGE] Max images reached, skipping\n");
        return false;
    }
    
    AzString resolved = resolve_url(data, url_str);
    CStr url_cstr = cstr_new_ref(&resolved);
    printf("[IMAGE] Loading: %s\n", cstr_ptr(&url_cstr));
    
    // Fetch the image file
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(resolved);
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("[IMAGE] Failed to fetch\n");
        cstr_free(&url_cstr);
        AzHttpError_delete(&result.Err.payload);
        return false;
    }
    
    AzHttpResponse response = result.Ok.payload;
    
    if (!AzHttpResponse_isSuccess(&response)) {
        printf("[IMAGE] HTTP error: %u\n", response.status_code);
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
    // Get the response body as bytes
    AzU8Vec body = response.body;
    
    if (body.len == 0) {
        printf("[IMAGE] Empty response body\n");
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
    // Copy bytes for decoding
    AzU8Vec image_bytes = AzU8Vec_clone(&body);
    
    // Decode the image (auto-detect format)
    AzResultRawImageDecodeImageError decode_result = AzRawImage_decodeImageBytesAny(
        (AzU8VecRef){ .ptr = image_bytes.ptr, .len = image_bytes.len }
    );
    
    AzU8Vec_delete(&image_bytes);
    
    if (decode_result.Err.tag == AzResultRawImageDecodeImageError_Tag_Err) {
        printf("[IMAGE] Failed to decode\n");
        cstr_free(&url_cstr);
        // AzDecodeImageError is a simple enum, no delete needed
        AzHttpResponse_delete(&response);
        return false;
    }
    
    AzRawImage raw_image = decode_result.Ok.payload;
    
    // Create ImageRef from RawImage
    AzOptionImageRef image_result = AzImageRef_rawImage(raw_image);
    
    if (AzOptionImageRef_isNone(&image_result)) {
        printf("[IMAGE] Failed to create ImageRef\n");
        cstr_free(&url_cstr);
        AzHttpResponse_delete(&response);
        return false;
    }
    
    // Store the ImageRef
    data->images[data->image_count] = image_result.Some.payload;
    data->image_urls[data->image_count] = strdup(cstr_ptr(&url_cstr));
    data->image_count++;
    
    printf("[IMAGE] Loaded successfully (%zu bytes, %ux%u)\n", 
           (size_t)body.len, (unsigned)raw_image.width, (unsigned)raw_image.height);
    
    cstr_free(&url_cstr);
    AzHttpResponse_delete(&response);
    return true;
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

// Load a local .xht/.xhtml/.html file
bool load_local_file(BrowserData* data) {
    browser_data_set_status(data, "Loading local file...");
    printf("\n[BROWSER] Loading local file: %s\n", data->url);
    
    // Read the file using Azul's file API
    AzFilePath file_path = { .inner = az_str(data->url) };
    AzResultStringFileError read_result = AzFilePath_readString(&file_path);
    
    if (read_result.Ok.tag == AzResultStringFileError_Tag_Err) {
        printf("[BROWSER] Failed to read file: %s\n", data->url);
        browser_data_set_error(data, "Failed to read file");
        AzFileError_delete(&read_result.Err.payload);
        return false;
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
        return false;
    }
    
    data->parsed_xml = xml_result.Ok.payload;
    data->has_xml = true;
    
    printf("[BROWSER] XHTML parsed successfully\n");
    browser_data_set_status(data, "Ready");
    
    data->is_loading = false;
    return true;
}

// ============================================================================
// Main Page Loading Logic
// ============================================================================

bool load_page(BrowserData* data) {
    // Check if this is a local file
    if (is_local_file(data->url)) {
        return load_local_file(data);
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
        return false;
    }
    
    // Fetch the HTML page
    AzResultHttpResponseHttpError http_result = AzHttpRequestConfig_httpGetDefault(az_str(data->url));
    
    printf("[BROWSER] HTTP result tag: %d (Ok=%d, Err=%d)\n", 
           http_result.Ok.tag, 
           AzResultHttpResponseHttpError_Tag_Ok,
           AzResultHttpResponseHttpError_Tag_Err);
    
    if (http_result.Ok.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("[BROWSER] HTTP error occurred\n");
        browser_data_set_error(data, "Failed to fetch page");
        AzHttpError_delete(&http_result.Err.payload);
        return false;
    }
    
    AzHttpResponse response = http_result.Ok.payload;
    
    printf("[BROWSER] Response status: %u, body len: %zu\n", response.status_code, (size_t)response.body.len);
    
    if (!AzHttpResponse_isSuccess(&response)) {
        char err[64];
        snprintf(err, sizeof(err), "HTTP Error: %u", response.status_code);
        browser_data_set_error(data, err);
        AzHttpResponse_delete(&response);
        return false;
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
        return false;
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
        return false;
    }
    
    data->parsed_xml = xml_result.Ok.payload;
    data->has_xml = true;
    
    printf("[BROWSER] HTML parsed successfully\n");
    browser_data_set_status(data, "Scanning for resources...");
    
    // Scan for external resources
    AzExternalResourceVec resources = AzXml_scanExternalResources(&data->parsed_xml);
    
    printf("[BROWSER] Found %zu external resources\n", (size_t)resources.len);
    
    // Download resources
    browser_data_set_status(data, "Loading resources...");
    
    size_t fonts_loaded = 0;
    size_t images_loaded = 0;
    size_t stylesheets_found = 0;
    
    for (size_t i = 0; i < resources.len; i++) {
        AzExternalResource* res = &((AzExternalResource*)resources.ptr)[i];
        
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
        
        // Load based on resource type
        switch (res->kind) {
            case AzExternalResourceKind_Font:
                if (load_font(data, &res->url)) fonts_loaded++;
                break;
            case AzExternalResourceKind_Image:
            case AzExternalResourceKind_Icon:
                if (load_image(data, &res->url)) images_loaded++;
                break;
            case AzExternalResourceKind_Stylesheet:
                stylesheets_found++;
                // TODO: Fetch and parse external CSS
                printf("  [STYLESHEET] External CSS not yet supported\n");
                break;
            default:
                // Skip scripts, video, audio for now
                break;
        }
        
        cstr_free(&url_cstr);
        cstr_free(&elem_cstr);
        cstr_free(&attr_cstr);
    }
    
    AzExternalResourceVec_delete(&resources);
    AzHttpResponse_delete(&response);
    
    printf("\n[BROWSER] Resources loaded: %zu fonts, %zu images\n", fonts_loaded, images_loaded);
    
    char status[128];
    snprintf(status, sizeof(status), "Loaded: %zu fonts, %zu images", fonts_loaded, images_loaded);
    browser_data_set_status(data, status);
    
    data->is_loading = false;
    return true;
}

// ============================================================================
// Layout Callback
// ============================================================================

AzStyledDom layout(AzRefAny data_ref, AzLayoutCallbackInfo info) {
    BrowserData* data = (BrowserData*)AzRefAny_getDataPtr(&data_ref);
    
    // If still loading or error, show status
    if (data->error_message != NULL) {
        // Show error message
        AzDom error_dom = AzDom_createDiv();
        AzDom text = AzDom_createText(az_str(data->error_message));
        AzDom_addChild(&error_dom, text);
        // Convert DOM to StyledDom using XML path
        AzStyledDom styled = AzStyledDom_default();
        return styled;
    }
    
    if (!data->has_xml) {
        // Show loading status
        AzDom loading_dom = AzDom_createDiv();
        AzDom text = AzDom_createText(az_str(data->status_message ? data->status_message : "Loading..."));
        AzDom_addChild(&loading_dom, text);
        AzStyledDom styled = AzStyledDom_default();
        return styled;
    }
    
    // Render the parsed XML as styled DOM using the new fromParsedXml function
    // This avoids re-parsing the XML string
    AzXml xml_clone = AzXml_clone(&data->parsed_xml);
    AzStyledDom styled = AzStyledDom_fromParsedXml(xml_clone);
    
    return styled;
}

// ============================================================================
// Destructor Callback
// ============================================================================

void browser_data_destructor(void* restrict ptr, BrowserData* restrict data) {
    (void)ptr;
    
    if (data->url) free(data->url);
    if (data->status_message) free(data->status_message);
    if (data->error_message) free(data->error_message);
    
    if (data->has_base_url) {
        AzUrl_delete(&data->base_url);
    }
    
    if (data->has_xml) {
        AzXml_delete(&data->parsed_xml);
    }
    
    for (size_t i = 0; i < data->font_count; i++) {
        AzFontRef_delete(&data->fonts[i]);
        if (data->font_names[i]) free(data->font_names[i]);
    }
    
    for (size_t i = 0; i < data->image_count; i++) {
        AzImageRef_delete(&data->images[i]);
        if (data->image_urls[i]) free(data->image_urls[i]);
    }
    
    free(data);
}

// ============================================================================
// Main
// ============================================================================

int main(int argc, char** argv) {
    const char* url = "https://example.com";
    
    if (argc > 1) {
        url = argv[1];
    }
    
    printf("=== Azul Simple Browser ===\n");
    printf("URL/File: %s\n\n", url);
    printf("Usage: ./browser <url or file.xht>\n");
    printf("  URL:  ./browser https://example.com\n");
    printf("  File: ./browser test.xht\n\n");
    printf("Note: This is a static browser demo without JavaScript support.\n");
    printf("It demonstrates fetching HTML, parsing it, downloading resources,\n");
    printf("and using FontRef/ImageRef for rendering.\n\n");
    
    // Create browser data
    BrowserData* data = browser_data_new(url);
    
    // Load the page (blocking for simplicity)
    // In a real app, this would be done in a background thread/task
    if (!load_page(data)) {
        printf("Failed to load page: %s\n", data->error_message ? data->error_message : "Unknown error");
    }
    
    // Create app
    AzString type_name = az_str("BrowserData");
    AzRefAny ref_data = AzRefAny_newC(
        (AzGlVoidPtrConst){ .ptr = data },
        sizeof(BrowserData),
        1,  // type_id
        0,
        type_name,
        (AzRefAnyDestructorType)browser_data_destructor,
        0,
        0
    );
    
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(ref_data, config);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    
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
