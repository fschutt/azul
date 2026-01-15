/**
 * Project Fluent Localization Demo for Azul GUI Framework
 * 
 * This example demonstrates:
 * - Syntax checking of .fluent files
 * - Loading translations from strings and bytes
 * - Message formatting with variables
 * - Language selection
 * 
 * Compile with: 
 *   gcc -o fluent fluent.c -I. -L../../target/release -lazul_dll -Wl,-rpath,../../target/release
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

CStr cstr_new(const AzString* s) {
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

#define WITH_CSTR(azstr, varname, code) do { \
    CStr _tmp_cstr = cstr_new(&(azstr)); \
    const char* varname = cstr_ptr(&_tmp_cstr); \
    code; \
    cstr_free(&_tmp_cstr); \
} while(0)

// ============================================================================
// Fluent Syntax Check Demo
// ============================================================================

void demo_syntax_check(void) {
    printf("\n============================================================\n");
    printf("Fluent Syntax Check Demo\n");
    printf("============================================================\n\n");
    
    // Valid FTL content
    printf("Checking valid FTL syntax:\n");
    const char* valid_ftl = 
        "hello = Hello, World!\n"
        "greeting = Hello, { $name }!\n"
        "emails = { $count ->\n"
        "    [one] You have one email.\n"
        "   *[other] You have { $count } emails.\n"
        "}\n";
    
    printf("  FTL content:\n");
    printf("  ---\n");
    printf("%s", valid_ftl);
    printf("  ---\n\n");
    
    AzFluentSyntaxCheckResult check = AzFluentSyntaxCheckResult_checkSyntax(az_str(valid_ftl));
    
    printf("  Result: %s\n", AzFluentSyntaxCheckResult_isOk(&check) ? "VALID" : "INVALID");
    
    AzOptionStringVec errors_opt = AzFluentSyntaxCheckResult_getErrors(&check);
    if (errors_opt.Some.tag == AzOptionStringVec_Tag_Some) {
        printf("  Errors: %zu\n", errors_opt.Some.payload.len);
        AzStringVec_delete(&errors_opt.Some.payload);
    } else {
        printf("  Errors: 0\n");
    }
    AzFluentSyntaxCheckResult_delete(&check);
    
    // Invalid FTL content
    printf("\nChecking invalid FTL syntax:\n");
    const char* invalid_ftl = 
        "hello = Hello\n"
        "broken = { $name\n"  // Missing closing brace
        "also-broken = \n";   // Empty value
    
    printf("  FTL content:\n");
    printf("  ---\n");
    printf("%s", invalid_ftl);
    printf("  ---\n\n");
    
    AzFluentSyntaxCheckResult check2 = AzFluentSyntaxCheckResult_checkSyntax(az_str(invalid_ftl));
    
    printf("  Result: %s\n", AzFluentSyntaxCheckResult_isOk(&check2) ? "VALID" : "INVALID");
    
    AzOptionStringVec errors2_opt = AzFluentSyntaxCheckResult_getErrors(&check2);
    if (errors2_opt.Some.tag == AzOptionStringVec_Tag_Some) {
        AzStringVec errors2 = errors2_opt.Some.payload;
        printf("  Errors: %zu\n", errors2.len);
        
        for (size_t i = 0; i < errors2.len; i++) {
            AzString* err = &((AzString*)errors2.ptr)[i];
            WITH_CSTR(*err, msg, {
                printf("    %zu. %s\n", i + 1, msg);
            });
        }
        
        AzStringVec_delete(&errors2);
    } else {
        printf("  Errors: 0\n");
    }
    AzFluentSyntaxCheckResult_delete(&check2);
}

// ============================================================================
// Basic Fluent Localization Demo
// ============================================================================

void demo_basic_localization(void) {
    printf("\n============================================================\n");
    printf("Basic Fluent Localization Demo\n");
    printf("============================================================\n\n");
    
    // Create English translations
    const char* en_ftl = 
        "app-name = My Application\n"
        "welcome = Welcome to { $app }!\n"
        "button-save = Save\n"
        "button-cancel = Cancel\n"
        "items-count = { $count ->\n"
        "    [one] { $count } item\n"
        "   *[other] { $count } items\n"
        "}\n"
        "user-greeting = Hello, { $name }! You have { $count } new messages.\n";
    
    // Create German translations
    const char* de_ftl = 
        "app-name = Meine Anwendung\n"
        "welcome = Willkommen bei { $app }!\n"
        "button-save = Speichern\n"
        "button-cancel = Abbrechen\n"
        "items-count = { $count ->\n"
        "    [one] { $count } Element\n"
        "   *[other] { $count } Elemente\n"
        "}\n"
        "user-greeting = Hallo, { $name }! Sie haben { $count } neue Nachrichten.\n";
    
    // Create localizer with English as default
    printf("Creating Fluent localizer with 2 languages...\n\n");
    
    AzFluentLocalizerHandle localizer = AzFluentLocalizerHandle_create(az_str("en-US"));
    
    // Add English translations
    bool en_ok = AzFluentLocalizerHandle_addResource(&localizer, az_str("en-US"), az_str(en_ftl));
    printf("  Added English (en-US) translations: %s\n", en_ok ? "OK" : "FAILED");
    
    // Add German translations
    bool de_ok = AzFluentLocalizerHandle_addResource(&localizer, az_str("de-DE"), az_str(de_ftl));
    printf("  Added German (de-DE) translations: %s\n", de_ok ? "OK" : "FAILED");
    
    // List available languages
    printf("\nAvailable locales:\n");
    AzStringVec locales = AzFluentLocalizerHandle_getLoadedLocales(&localizer);
    for (size_t i = 0; i < locales.len; i++) {
        AzString* locale = &((AzString*)locales.ptr)[i];
        WITH_CSTR(*locale, loc, {
            printf("  - %s\n", loc);
        });
    }
    AzStringVec_delete(&locales);
    
    // Translate simple messages
    printf("\nSimple translations:\n");
    
    // No arguments - use empty FmtArgVec
    AzFmtArgVec empty_args = { .ptr = NULL, .len = 0, .cap = 0, .destructor = AzFmtArgVecDestructor_noDestructor() };
    
    AzString app_name_en = AzFluentLocalizerHandle_translate(&localizer, az_str("en-US"), az_str("app-name"), empty_args);
    WITH_CSTR(app_name_en, name, {
        printf("  app-name (en-US): %s\n", name);
    });
    AzString_delete(&app_name_en);
    
    AzString app_name_de = AzFluentLocalizerHandle_translate(&localizer, az_str("de-DE"), az_str("app-name"), empty_args);
    WITH_CSTR(app_name_de, name, {
        printf("  app-name (de-DE): %s\n", name);
    });
    AzString_delete(&app_name_de);
    
    // Check if message exists
    printf("\nMessage existence check:\n");
    bool has_save = AzFluentLocalizerHandle_hasMessage(&localizer, az_str("en-US"), az_str("button-save"));
    bool has_missing = AzFluentLocalizerHandle_hasMessage(&localizer, az_str("en-US"), az_str("nonexistent"));
    printf("  button-save exists: %s\n", has_save ? "yes" : "no");
    printf("  nonexistent exists: %s\n", has_missing ? "yes" : "no");
    
    AzFluentLocalizerHandle_delete(&localizer);
}

// ============================================================================
// Syntax Check from Bytes Demo (for CI usage)
// ============================================================================

void demo_syntax_check_bytes(void) {
    printf("\n============================================================\n");
    printf("Syntax Check from Bytes Demo (CI Usage)\n");
    printf("============================================================\n\n");
    
    const char* ftl_content = 
        "# This is a valid Fluent file\n"
        "hello = Hello, World!\n"
        "greeting = Hello, { $name }!\n";
    
    printf("Checking syntax from bytes (simulating file read)...\n");
    
    AzU8Vec bytes = AzU8Vec_copyFromBytes((const uint8_t*)ftl_content, 0, strlen(ftl_content));
    AzU8VecRef bytes_ref = AzU8Vec_asRefVec(&bytes);
    
    AzFluentSyntaxCheckResult result = AzFluentSyntaxCheckResult_checkSyntaxBytes(bytes_ref);
    
    if (AzFluentSyntaxCheckResult_isOk(&result)) {
        printf("  Result: VALID - file can be used\n");
    } else {
        printf("  Result: INVALID - errors found:\n");
        AzOptionStringVec errs_opt = AzFluentSyntaxCheckResult_getErrors(&result);
        if (errs_opt.Some.tag == AzOptionStringVec_Tag_Some) {
            AzStringVec errs = errs_opt.Some.payload;
            for (size_t i = 0; i < errs.len; i++) {
                AzString* err = &((AzString*)errs.ptr)[i];
                WITH_CSTR(*err, msg, {
                    printf("    %s\n", msg);
                });
            }
            AzStringVec_delete(&errs);
        }
    }
    
    AzFluentSyntaxCheckResult_delete(&result);
    AzU8Vec_delete(&bytes);
}

// ============================================================================
// Main
// ============================================================================

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    
    printf("Azul Fluent Localization Demo\n");
    printf("==============================\n");
    
    demo_syntax_check();
    demo_basic_localization();
    demo_syntax_check_bytes();
    
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    
    return 0;
}
