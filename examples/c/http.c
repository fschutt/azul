/**
 * HTTP Client Demo for Azul GUI Framework
 * 
 * This example demonstrates:
 * - Simple HTTP GET requests
 * - HTTP requests with custom configuration
 * - URL parsing and manipulation
 * - Error handling for HTTP operations
 * 
 * Compile with: 
 *   gcc -o http http.c -I. -L../../target/release -lazul_dll -Wl,-rpath,../../target/release
 * 
 * Note: The azul-dll must be compiled with the 'http' feature:
 *   cargo build -p azul-dll --features http,build-dll --release
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

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
// URL Parsing Demo
// ============================================================================

void demo_url_parsing(void) {
    printf("\n============================================================\n");
    printf("URL Parsing Demo\n");
    printf("============================================================\n\n");
    
    // Parse a full URL
    AzResultUrlUrlParseError result = AzUrl_parse(az_str("https://api.example.com:8080/v1/data?format=json#results"));
    
    if (result.Err.tag == AzResultUrlUrlParseError_Tag_Err) {
        CStr err = cstr_new_ref(&result.Err.payload.message);
        printf("URL parse error: %s\n", cstr_ptr(&err));
        cstr_free(&err);
        AzUrlParseError_delete(&result.Err.payload);
        return;
    }
    
    AzUrl url = result.Ok.payload;
    
    printf("Parsed URL components:\n");
    CStr href = cstr_new(AzString_clone(&url.href));
    CStr scheme = cstr_new(AzString_clone(&url.scheme));
    CStr host = cstr_new(AzString_clone(&url.host));
    CStr path = cstr_new(AzString_clone(&url.path));
    CStr query = cstr_new(AzString_clone(&url.query));
    CStr fragment = cstr_new(AzString_clone(&url.fragment));
    
    printf("  Full URL:  %s\n", cstr_ptr(&href));
    printf("  Scheme:    %s\n", cstr_ptr(&scheme));
    printf("  Host:      %s\n", cstr_ptr(&host));
    printf("  Port:      %u\n", url.port);
    printf("  Path:      %s\n", cstr_ptr(&path));
    printf("  Query:     %s\n", cstr_ptr(&query));
    printf("  Fragment:  %s\n", cstr_ptr(&fragment));
    
    cstr_free(&href);
    cstr_free(&scheme);
    cstr_free(&host);
    cstr_free(&path);
    cstr_free(&query);
    cstr_free(&fragment);
    
    // Test URL methods
    printf("\n  is_https:  %s\n", AzUrl_isHttps(&url) ? "true" : "false");
    printf("  is_http:   %s\n", AzUrl_isHttp(&url) ? "true" : "false");
    printf("  eff. port: %u\n", AzUrl_effectivePort(&url));
    
    // Join a relative path
    printf("\nJoining relative path '/v2/users':\n");
    AzResultUrlUrlParseError join_result = AzUrl_join(&url, az_str("/v2/users"));
    if (join_result.Ok.tag == AzResultUrlUrlParseError_Tag_Ok) {
        CStr joined = cstr_new(AzString_clone(&join_result.Ok.payload.href));
        printf("  Joined URL: %s\n", cstr_ptr(&joined));
        cstr_free(&joined);
        AzUrl_delete(&join_result.Ok.payload);
    }
    
    AzUrl_delete(&url);
    
    // Create URL from parts
    printf("\nCreating URL from parts:\n");
    AzUrl built = AzUrl_fromParts(az_str("https"), az_str("example.com"), 443, az_str("/api/data"));
    CStr built_href = cstr_new(AzString_clone(&built.href));
    printf("  Built URL: %s\n", cstr_ptr(&built_href));
    cstr_free(&built_href);
    AzUrl_delete(&built);
}

// ============================================================================
// HTTP Request Demo
// ============================================================================

void demo_http_get(void) {
    printf("\n============================================================\n");
    printf("HTTP GET Request Demo\n");
    printf("============================================================\n\n");
    
    printf("Fetching https://httpbin.org/get ...\n\n");
    
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(az_str("https://httpbin.org/get"));
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    
    printf("Response received:\n");
    printf("  Status code:    %u\n", response.status_code);
    printf("  Content length: %llu bytes\n", (unsigned long long)response.content_length);
    
    CStr ct = cstr_new(AzString_clone(&response.content_type));
    printf("  Content type:   %s\n", cstr_ptr(&ct));
    cstr_free(&ct);
    
    printf("  Is success:     %s\n", AzHttpResponse_isSuccess(&response) ? "true" : "false");
    printf("  Is redirect:    %s\n", AzHttpResponse_isRedirect(&response) ? "true" : "false");
    
    // Print headers
    printf("\n  Headers (%zu):\n", response.headers.len);
    for (size_t i = 0; i < response.headers.len && i < 5; i++) {
        AzHttpHeader* hdr = &((AzHttpHeader*)response.headers.ptr)[i];
        CStr name = cstr_new(AzString_clone(&hdr->name));
        CStr value = cstr_new(AzString_clone(&hdr->value));
        printf("    %s: %s\n", cstr_ptr(&name), cstr_ptr(&value));
        cstr_free(&name);
        cstr_free(&value);
    }
    
    // Print body preview
    AzOptionString body_str = AzHttpResponse_bodyAsString(&response);
    if (body_str.Some.tag == AzOptionString_Tag_Some) {
        CStr body = cstr_new(body_str.Some.payload);
        const char* body_ptr = cstr_ptr(&body);
        size_t len = strlen(body_ptr);
        if (len > 200) {
            printf("\n  Body preview (first 200 chars):\n    %.200s...\n", body_ptr);
        } else {
            printf("\n  Body:\n    %s\n", body_ptr);
        }
        cstr_free(&body);
    }
    
    AzHttpResponse_delete(&response);
}

void demo_http_with_config(void) {
    printf("\n============================================================\n");
    printf("HTTP Request with Custom Configuration\n");
    printf("============================================================\n\n");
    
    // Create custom configuration using builder pattern (by-value)
    AzHttpRequestConfig config = AzHttpRequestConfig_new();
    config = AzHttpRequestConfig_withTimeout(config, 10);
    config = AzHttpRequestConfig_withMaxSize(config, 1024 * 1024);
    config = AzHttpRequestConfig_withUserAgent(config, az_str("AzulApp/1.0 (C Example)"));
    
    printf("Configuration:\n");
    printf("  Timeout:       %llu seconds\n", (unsigned long long)config.timeout_secs);
    printf("  Max size:      %llu bytes\n", (unsigned long long)config.max_response_size);
    CStr ua = cstr_new(AzString_clone(&config.user_agent));
    printf("  User-Agent:    %s\n", cstr_ptr(&ua));
    cstr_free(&ua);
    
    printf("\nFetching https://httpbin.org/headers ...\n");
    
    AzResultHttpResponseHttpError response_result = AzHttpRequestConfig_httpGet(&config, az_str("https://httpbin.org/headers"));
    AzHttpRequestConfig_delete(&config);
    
    if (response_result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("Request failed!\n");
        AzHttpError_delete(&response_result.Err.payload);
        return;
    }
    
    printf("\nRequest successful! Status: %u\n", response_result.Ok.payload.status_code);
    
    AzOptionString body_str = AzHttpResponse_bodyAsString(&response_result.Ok.payload);
    if (body_str.Some.tag == AzOptionString_Tag_Some) {
        printf("Response body:\n");
        CStr body = cstr_new(body_str.Some.payload);
        printf("%s\n", cstr_ptr(&body));
        cstr_free(&body);
    }
    
    AzHttpResponse_delete(&response_result.Ok.payload);
}

void demo_download_bytes(void) {
    printf("\n============================================================\n");
    printf("Download Bytes Demo\n");
    printf("============================================================\n\n");
    
    printf("Downloading a small image from httpbin.org...\n");
    
    AzResultU8VecHttpError result = AzHttpRequestConfig_downloadBytesDefault(az_str("https://httpbin.org/image/png"));
    
    if (result.Err.tag == AzResultU8VecHttpError_Tag_Err) {
        printf("Download failed!\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzU8Vec bytes = result.Ok.payload;
    printf("Downloaded %zu bytes\n", bytes.len);
    
    // Check PNG magic bytes
    if (bytes.len >= 8 && 
        bytes.ptr[0] == 0x89 && bytes.ptr[1] == 'P' && 
        bytes.ptr[2] == 'N' && bytes.ptr[3] == 'G') {
        printf("Verified: Valid PNG file (magic bytes: 89 50 4E 47)\n");
    }
    
    AzU8Vec_delete(&bytes);
}

void demo_url_reachability(void) {
    printf("\n============================================================\n");
    printf("URL Reachability Check Demo\n");
    printf("============================================================\n\n");
    
    const char* urls[] = {
        "https://httpbin.org/status/200",
        "https://httpbin.org/status/404",
        "https://this-domain-does-not-exist.invalid/",
    };
    const char* descriptions[] = {
        "Should succeed (200 OK)",
        "Should fail (404 Not Found)",
        "Should fail (DNS error)",
    };
    
    for (int i = 0; i < 3; i++) {
        printf("Checking: %s\n", urls[i]);
        printf("  Expected: %s\n", descriptions[i]);
        
        bool reachable = AzHttpRequestConfig_isUrlReachable(az_str(urls[i]));
        
        printf("  Result:   %s\n\n", reachable ? "REACHABLE" : "NOT REACHABLE");
    }
}

// ============================================================================
// Main
// ============================================================================

int main(void) {
    printf("Azul HTTP Client Demo\n");
    printf("======================\n");
    
    demo_url_parsing();
    demo_http_get();
    demo_http_with_config();
    demo_download_bytes();
    demo_url_reachability();
    
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    
    return 0;
}
