/**
 * HTTP Client Demo for Azul GUI Framework
 *
 * This example demonstrates:
 * - Simple HTTP GET requests
 * - HTTP requests with custom configuration
 * - URL parsing and manipulation
 * - Error handling for HTTP operations
 *
 * HTTP is asynchronous: AzHttpRequestConfig_httpGet / _downloadBytes /
 * _isUrlReachable only REQUEST the transfer and deliver the result later,
 * through the event loop, to a resume callback (a browser can only answer
 * these asynchronously). So this demo is a tiny azul app: main() runs the
 * synchronous URL parsing, then starts an app whose layout callback issues
 * the first request exactly once; every resume prints its result and issues
 * the next request, and the last one exits the process so the demo still
 * behaves like a CLI tool.
 *
 * Compile with:
 *   gcc -o http http.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 *
 * Note: The azul-dll must be compiled with the 'http' feature:
 *   cargo build -p azul-dll --features http,build-dll --release
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
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
// Demo State
// ============================================================================

typedef struct {
    bool started;       // the layout callback issued the first request
    int reach_index;    // next URL of the reachability demo to check
} HttpDemo;

void HttpDemo_destructor(void* p) { (void)p; }
AZ_REFLECT(HttpDemo, HttpDemo_destructor);

// The steps of the chain, in the order they run
void demo_http_get(AzRefAny data);
AzUpdate on_http_get(AzRefAny data, AzCallbackInfo info, AzRefAny result);
void demo_http_with_config(AzRefAny data);
AzUpdate on_http_with_config(AzRefAny data, AzCallbackInfo info, AzRefAny result);
void demo_download_bytes(AzRefAny data);
AzUpdate on_download_bytes(AzRefAny data, AzCallbackInfo info, AzRefAny result);
void demo_url_reachability(AzRefAny data);
static void check_next_url(AzRefAny data);
AzUpdate on_url_checked(AzRefAny data, AzCallbackInfo info, AzRefAny result);
static void finish_demo(void);

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

// Issue the GET; the response arrives in on_http_get
void demo_http_get(AzRefAny data) {
    printf("\n============================================================\n");
    printf("HTTP GET Request Demo\n");
    printf("============================================================\n\n");

    printf("Fetching https://httpbin.org/get ...\n\n");

    // A default configuration is enough for a plain GET
    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_httpGet(&config, az_str("https://httpbin.org/get"), AzRefAny_clone(&data), on_http_get);
    AzHttpRequestConfig_delete(&config);
}

AzUpdate on_http_get(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    (void)info;

    AzOptionHttpGetResult r = AzHttpGetResult_downcast(result);
    if (r.Some.tag != AzOptionHttpGetResult_Tag_Some) {
        printf("HTTP request failed: unexpected result payload\n");
    } else if (r.Some.payload.result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&r.Some.payload.result.Err.payload);
    } else {
        AzHttpResponse response = r.Some.payload.result.Ok.payload;

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

    demo_http_with_config(data);
    return AzUpdate_DoNothing;
}

// Issue the GET with a custom configuration; the response arrives in on_http_with_config
void demo_http_with_config(AzRefAny data) {
    printf("\n============================================================\n");
    printf("HTTP Request with Custom Configuration\n");
    printf("============================================================\n\n");

    // Create custom configuration using builder pattern (by-value)
    AzHttpRequestConfig config = AzHttpRequestConfig_create();
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

    AzHttpRequestConfig_httpGet(&config, az_str("https://httpbin.org/headers"), AzRefAny_clone(&data), on_http_with_config);
    AzHttpRequestConfig_delete(&config);
}

AzUpdate on_http_with_config(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    (void)info;

    AzOptionHttpGetResult r = AzHttpGetResult_downcast(result);
    if (r.Some.tag != AzOptionHttpGetResult_Tag_Some) {
        printf("Request failed: unexpected result payload\n");
    } else if (r.Some.payload.result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("Request failed!\n");
        AzHttpError_delete(&r.Some.payload.result.Err.payload);
    } else {
        AzHttpResponse response = r.Some.payload.result.Ok.payload;

        printf("\nRequest successful! Status: %u\n", response.status_code);

        AzOptionString body_str = AzHttpResponse_bodyAsString(&response);
        if (body_str.Some.tag == AzOptionString_Tag_Some) {
            printf("Response body:\n");
            CStr body = cstr_new(body_str.Some.payload);
            printf("%s\n", cstr_ptr(&body));
            cstr_free(&body);
        }

        AzHttpResponse_delete(&response);
    }

    demo_download_bytes(data);
    return AzUpdate_DoNothing;
}

// Issue the download; the bytes arrive in on_download_bytes
void demo_download_bytes(AzRefAny data) {
    printf("\n============================================================\n");
    printf("Download Bytes Demo\n");
    printf("============================================================\n\n");

    printf("Downloading a small image from httpbin.org...\n");

    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_downloadBytes(&config, az_str("https://httpbin.org/image/png"), AzRefAny_clone(&data), on_download_bytes);
    AzHttpRequestConfig_delete(&config);
}

AzUpdate on_download_bytes(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    (void)info;

    AzOptionHttpBytesResult r = AzHttpBytesResult_downcast(result);
    if (r.Some.tag != AzOptionHttpBytesResult_Tag_Some) {
        printf("Download failed: unexpected result payload\n");
    } else if (r.Some.payload.result.Err.tag == AzResultU8VecHttpError_Tag_Err) {
        printf("Download failed!\n");
        AzHttpError_delete(&r.Some.payload.result.Err.payload);
    } else {
        AzU8Vec bytes = r.Some.payload.result.Ok.payload;
        printf("Downloaded %zu bytes\n", bytes.len);

        // Check PNG magic bytes
        if (bytes.len >= 8 &&
            bytes.ptr[0] == 0x89 && bytes.ptr[1] == 'P' &&
            bytes.ptr[2] == 'N' && bytes.ptr[3] == 'G') {
            printf("Verified: Valid PNG file (magic bytes: 89 50 4E 47)\n");
        }

        AzU8Vec_delete(&bytes);
    }

    demo_url_reachability(data);
    return AzUpdate_DoNothing;
}

// The reachability checks run one after the other: each resume issues the next
static const char* REACH_URLS[] = {
    "https://httpbin.org/status/200",
    "https://httpbin.org/status/404",
    "https://this-domain-does-not-exist.invalid/",
};
static const char* REACH_DESCRIPTIONS[] = {
    "Should succeed (200 OK)",
    "Should fail (404 Not Found)",
    "Should fail (DNS error)",
};
#define REACH_COUNT 3

void demo_url_reachability(AzRefAny data) {
    printf("\n============================================================\n");
    printf("URL Reachability Check Demo\n");
    printf("============================================================\n\n");

    HttpDemoRefMut d = HttpDemoRefMut_create(&data);
    if (!HttpDemo_downcastMut(&data, &d)) {
        finish_demo();
        return;
    }
    d.ptr->reach_index = 0;
    HttpDemoRefMut_delete(&d);

    check_next_url(data);
}

// Issue the check for the current URL, or finish once all were checked
static void check_next_url(AzRefAny data) {
    int i = REACH_COUNT;
    HttpDemoRef d = HttpDemoRef_create(&data);
    if (HttpDemo_downcastRef(&data, &d)) {
        i = d.ptr->reach_index;
        HttpDemoRef_delete(&d);
    }

    if (i >= REACH_COUNT) {
        finish_demo();
        return;
    }

    printf("Checking: %s\n", REACH_URLS[i]);
    printf("  Expected: %s\n", REACH_DESCRIPTIONS[i]);

    AzHttpRequestConfig config = AzHttpRequestConfig_create();
    AzHttpRequestConfig_isUrlReachable(&config, az_str(REACH_URLS[i]), AzRefAny_clone(&data), on_url_checked);
    AzHttpRequestConfig_delete(&config);
}

AzUpdate on_url_checked(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    (void)info;

    AzOptionHttpReachableResult r = AzHttpReachableResult_downcast(result);
    if (r.Some.tag != AzOptionHttpReachableResult_Tag_Some) {
        printf("  Result:   unexpected result payload\n\n");
    } else {
        AzHttpReachableResult reach = r.Some.payload;
        printf("  Result:   %s\n", reach.reachable ? "REACHABLE" : "NOT REACHABLE");
        if (reach.error.Some.tag == AzOptionString_Tag_Some) {
            CStr err = cstr_new(reach.error.Some.payload);
            printf("  Error:    %s\n", cstr_ptr(&err));
            cstr_free(&err);
        }
        printf("\n");
    }

    HttpDemoRefMut d = HttpDemoRefMut_create(&data);
    if (HttpDemo_downcastMut(&data, &d)) {
        d.ptr->reach_index += 1;
        HttpDemoRefMut_delete(&d);
    }

    check_next_url(data);
    return AzUpdate_DoNothing;
}

// The whole chain ran: leave the event loop the way a CLI tool would
static void finish_demo(void) {
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    exit(0);
}

// ============================================================================
// Layout Callback
// ============================================================================

// Issues the first request exactly once; every later step is issued by the
// resume callback of the step before it.
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    HttpDemoRefMut d = HttpDemoRefMut_create(&data);
    if (HttpDemo_downcastMut(&data, &d)) {
        bool start = !d.ptr->started;
        d.ptr->started = true;
        HttpDemoRefMut_delete(&d);
        if (start) {
            demo_http_get(data);
        }
    }

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, AzDom_createPWithText(az_str("Azul HTTP demo - the output is on the console")));
    return body;
}

// ============================================================================
// Main
// ============================================================================

int main(void) {
    printf("Azul HTTP Client Demo\n");
    printf("======================\n");

    // URL parsing is pure and synchronous: run it before the app starts
    demo_url_parsing();

    // The requests need the event loop: run the rest as an app
    HttpDemo state = { .started = false, .reach_index = 0 };
    AzRefAny data = HttpDemo_upcast(state);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = az_str("Azul HTTP Demo");
    window.window_state.size.dimensions.width = 420.0;
    window.window_state.size.dimensions.height = 120.0;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);   // the last resume calls exit(0)
    AzApp_delete(&app);

    return 0;
}
