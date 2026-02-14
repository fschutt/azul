/**
 * JSON Query Demo for Azul GUI Framework
 * 
 * This example ports the jq tutorial from https://jqlang.org/tutorial/
 * It demonstrates:
 * - HTTP GET requests to fetch JSON from GitHub API
 * - JSON parsing and pretty printing
 * - JSON pointer queries (similar to jq expressions)
 * - Wildcard queries with jq_all()
 * 
 * Compile with: 
 *   clang -o jq jq.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
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

const char* cstr_ptr(const CStr* c) {
    return (const char*)c->vec.ptr;
}

void cstr_free(CStr* c) {
    AzU8Vec_delete(&c->vec);
}

// Print a separator line
void print_separator(const char* title) {
    printf("\n");
    printf("============================================================\n");
    printf("%s\n", title);
    printf("============================================================\n\n");
}

// ============================================================================
// Demo 1: Fetch and pretty-print JSON (like: curl ... | jq '.')
// ============================================================================

void demo_pretty_print(void) {
    print_separator("Demo 1: Pretty-print JSON (jq '.')");
    
    printf("Fetching: https://api.github.com/repos/jqlang/jq/commits?per_page=5\n\n");
    
    // Make HTTP GET request
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(
        az_str("https://api.github.com/repos/jqlang/jq/commits?per_page=5")
    );
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    printf("Status: %u\n", response.status_code);
    printf("Content-Length: %llu bytes\n\n", (unsigned long long)response.content_length);
    
    // Parse response body as JSON
    AzU8VecRef body_ref = AzU8Vec_asRefVec(&response.body);
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(body_ref);
    AzHttpResponse_delete(&response);
    
    if (parse_result.Err.tag == AzResultJsonJsonParseError_Tag_Err) {
        printf("JSON parse error\n");
        AzJsonParseError_delete(&parse_result.Err.payload);
        return;
    }
    
    AzJson json = parse_result.Ok.payload;
    
    // Pretty-print the JSON (like jq '.')
    CStr pretty = cstr_new(AzJson_toStringPretty(&json));
    const char* json_str = cstr_ptr(&pretty);
    size_t len = strlen(json_str);
    
    if (len > 2000) {
        printf("%.2000s\n...(truncated, %zu total bytes)\n", json_str, len);
    } else {
        printf("%s\n", json_str);
    }
    
    cstr_free(&pretty);
    AzJson_delete(&json);
}

// ============================================================================
// Demo 2: Get first element (like: jq '.[0]')
// ============================================================================

void demo_first_element(void) {
    print_separator("Demo 2: Get first commit (jq '.[0]')");
    
    printf("Query: .[0] (first array element)\n\n");
    
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(
        az_str("https://api.github.com/repos/jqlang/jq/commits?per_page=5")
    );
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    AzU8VecRef body_ref = AzU8Vec_asRefVec(&response.body);
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(body_ref);
    AzHttpResponse_delete(&response);
    
    if (parse_result.Err.tag == AzResultJsonJsonParseError_Tag_Err) {
        printf("JSON parse error\n");
        AzJsonParseError_delete(&parse_result.Err.payload);
        return;
    }
    
    AzJson json = parse_result.Ok.payload;
    
    // Use jq() to get first element: /0
    AzJson first = AzJson_jq(&json, az_str("/0"));
    AzJson_delete(&json);
    
    if (AzJson_isNull(&first)) {
        printf("Element not found\n");
        AzJson_delete(&first);
        return;
    }
    
    CStr pretty = cstr_new(AzJson_toStringPretty(&first));
    const char* json_str = cstr_ptr(&pretty);
    size_t len = strlen(json_str);
    
    if (len > 3000) {
        printf("%.3000s\n...(truncated)\n", json_str);
    } else {
        printf("%s\n", json_str);
    }
    
    cstr_free(&pretty);
    AzJson_delete(&first);
}

// ============================================================================
// Demo 3: Extract specific fields (like: jq '.[0] | {message: .commit.message}')
// ============================================================================

void demo_extract_fields(void) {
    print_separator("Demo 3: Extract commit message and author");
    
    printf("Equivalent jq: .[0] | {message: .commit.message, name: .commit.committer.name}\n\n");
    
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(
        az_str("https://api.github.com/repos/jqlang/jq/commits?per_page=5")
    );
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    AzU8VecRef body_ref = AzU8Vec_asRefVec(&response.body);
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(body_ref);
    AzHttpResponse_delete(&response);
    
    if (parse_result.Err.tag == AzResultJsonJsonParseError_Tag_Err) {
        printf("JSON parse error\n");
        AzJsonParseError_delete(&parse_result.Err.payload);
        return;
    }
    
    AzJson json = parse_result.Ok.payload;
    
    // Get first commit's message and author using jq()
    AzJson msg = AzJson_jq(&json, az_str("/0/commit/message"));
    AzJson name = AzJson_jq(&json, az_str("/0/commit/committer/name"));
    AzJson_delete(&json);
    
    printf("First commit:\n");
    
    if (!AzJson_isNull(&msg)) {
        CStr msg_str = cstr_new(AzJson_toString(&msg));
        printf("  message: %s\n", cstr_ptr(&msg_str));
        cstr_free(&msg_str);
    }
    AzJson_delete(&msg);
    
    if (!AzJson_isNull(&name)) {
        CStr name_str = cstr_new(AzJson_toString(&name));
        printf("  name: %s\n", cstr_ptr(&name_str));
        cstr_free(&name_str);
    }
    AzJson_delete(&name);
}

// ============================================================================
// Demo 4: Use wildcard to iterate (like: jq '.[].commit.message')
// ============================================================================

void demo_wildcard_iterate(void) {
    print_separator("Demo 4: Wildcard iteration with jq_all()");
    
    printf("Equivalent jq: .[].commit.message\n");
    printf("Using: jq_all(\"/*/commit/message\")\n\n");
    
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(
        az_str("https://api.github.com/repos/jqlang/jq/commits?per_page=5")
    );
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    AzU8VecRef body_ref = AzU8Vec_asRefVec(&response.body);
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(body_ref);
    AzHttpResponse_delete(&response);
    
    if (parse_result.Err.tag == AzResultJsonJsonParseError_Tag_Err) {
        printf("JSON parse error\n");
        AzJsonParseError_delete(&parse_result.Err.payload);
        return;
    }
    
    AzJson json = parse_result.Ok.payload;
    
    // Use jq_all() with wildcard to get all commit messages
    AzJsonVec messages = AzJson_jqAll(&json, az_str("/*/commit/message"));
    AzJson_delete(&json);
    
    printf("All commit messages (%zu found):\n\n", messages.len);
    
    for (size_t i = 0; i < messages.len; i++) {
        AzJson* msg = &((AzJson*)messages.ptr)[i];
        CStr msg_str = cstr_new(AzJson_toString(msg));
        const char* s = cstr_ptr(&msg_str);
        
        // Truncate long messages
        size_t len = strlen(s);
        if (len > 80) {
            printf("  %zu. %.77s...\n", i + 1, s);
        } else {
            printf("  %zu. %s\n", i + 1, s);
        }
        cstr_free(&msg_str);
    }
    
    AzJsonVec_delete(&messages);
}

// ============================================================================
// Demo 5: Nested wildcard (like: jq '.[].parents[].html_url')
// ============================================================================

void demo_nested_wildcard(void) {
    print_separator("Demo 5: Nested wildcards");
    
    printf("Equivalent jq: .[0].parents[].html_url\n");
    printf("Using: jq_all(\"/0/parents/*/html_url\")\n\n");
    
    AzResultHttpResponseHttpError result = AzHttpRequestConfig_httpGetDefault(
        az_str("https://api.github.com/repos/jqlang/jq/commits?per_page=5")
    );
    
    if (result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
        printf("HTTP request failed\n");
        AzHttpError_delete(&result.Err.payload);
        return;
    }
    
    AzHttpResponse response = result.Ok.payload;
    AzU8VecRef body_ref = AzU8Vec_asRefVec(&response.body);
    AzResultJsonJsonParseError parse_result = AzJson_parseBytes(body_ref);
    AzHttpResponse_delete(&response);
    
    if (parse_result.Err.tag == AzResultJsonJsonParseError_Tag_Err) {
        printf("JSON parse error\n");
        AzJsonParseError_delete(&parse_result.Err.payload);
        return;
    }
    
    AzJson json = parse_result.Ok.payload;
    
    // Get all parent URLs from first commit
    AzJsonVec parent_urls = AzJson_jqAll(&json, az_str("/0/parents/*/html_url"));
    AzJson_delete(&json);
    
    printf("First commit's parent URLs (%zu found):\n", parent_urls.len);
    
    for (size_t i = 0; i < parent_urls.len; i++) {
        AzJson* url = &((AzJson*)parent_urls.ptr)[i];
        CStr url_str = cstr_new(AzJson_toString(url));
        printf("  - %s\n", cstr_ptr(&url_str));
        cstr_free(&url_str);
    }
    
    AzJsonVec_delete(&parent_urls);
}

// ============================================================================
// Main
// ============================================================================

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    
    printf("JSON Query Demo - Porting jq Tutorial to Azul\n");
    printf("Based on: https://jqlang.org/tutorial/\n");
    printf("Using GitHub API: https://api.github.com/repos/jqlang/jq/commits\n");
    
    // Run all demos
    demo_pretty_print();
    demo_first_element();
    demo_extract_fields();
    demo_wildcard_iterate();
    demo_nested_wildcard();
    
    printf("\n");
    print_separator("Demo Complete!");
    printf("The Azul JSON query functions provide jq-like querying:\n\n");
    printf("Single value queries (jq):\n");
    printf("  jq '.'           -> AzJson_toStringPretty(&json)\n");
    printf("  jq '.[0]'        -> AzJson_jq(&json, \"/0\")\n");
    printf("  jq '.foo.bar'    -> AzJson_jq(&json, \"/foo/bar\")\n");
    printf("\nWildcard queries (jq_all):\n");
    printf("  jq '.[]'         -> AzJson_jqAll(&json, \"/*\")\n");
    printf("  jq '.[].name'    -> AzJson_jqAll(&json, \"/*/name\")\n");
    printf("  jq '.[].x[].y'   -> AzJson_jqAll(&json, \"/*/x/*/y\")\n");
    
    return 0;
}
