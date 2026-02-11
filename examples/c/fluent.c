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
 *   gcc -o fluent fluent.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
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
// Language Pack Demo (Download + Cache)
// ============================================================================

void demo_language_packs(void) {
    printf("\n============================================================\n");
    printf("Language Pack Demo (Download + Cache)\n");
    printf("============================================================\n\n");
    
    // In a real application, you would:
    // 1. Check if language pack is cached locally
    // 2. If not, download it from your server
    // 3. Cache it for offline use
    // 4. Load it into the localizer
    
    // Setup cache directory
    AzFilePath temp = AzFilePath_getTempDir();
    AzFilePath cache_dir = AzFilePath_joinStr(&temp, az_str("azul_lang_cache"));
    
    // Create cache directory
    AzResultVoidFileError dir_result = AzFilePath_createDirAll(&cache_dir);
    (void)dir_result; // Ignore if already exists
    
    AzString cache_path_str = AzFilePath_asString(&cache_dir);
    WITH_CSTR(cache_path_str, path, {
        printf("Language pack cache directory: %s\n\n", path);
    });
    AzString_delete(&cache_path_str);
    
    // Create localizer
    AzFluentLocalizerHandle localizer = AzFluentLocalizerHandle_create(az_str("en-US"));
    
    // Simulate loading language packs
    struct {
        const char* locale;
        const char* filename;
        const char* content;
    } lang_packs[] = {
        {
            "en-US",
            "en-US.ftl",
            "app-name = My Application\n"
            "greeting = Hello, { $name }!\n"
            "items = { $count ->\n"
            "    [one] { $count } item\n"
            "   *[other] { $count } items\n"
            "}\n"
        },
        {
            "de-DE",
            "de-DE.ftl",
            "app-name = Meine Anwendung\n"
            "greeting = Hallo, { $name }!\n"
            "items = { $count ->\n"
            "    [one] { $count } Element\n"
            "   *[other] { $count } Elemente\n"
            "}\n"
        },
        {
            "fr-FR",
            "fr-FR.ftl",
            "app-name = Mon Application\n"
            "greeting = Bonjour, { $name }!\n"
            "items = { $count ->\n"
            "    [one] { $count } élément\n"
            "   *[other] { $count } éléments\n"
            "}\n"
        },
    };
    
    size_t num_packs = sizeof(lang_packs) / sizeof(lang_packs[0]);
    
    printf("Loading %zu language packs:\n", num_packs);
    
    for (size_t i = 0; i < num_packs; i++) {
        // Check if cached
        AzFilePath pack_path = AzFilePath_joinStr(&cache_dir, az_str(lang_packs[i].filename));
        bool cached = AzFilePath_exists(&pack_path);
        
        if (cached) {
            // Load from cache
            printf("  %s: Loading from cache...", lang_packs[i].locale);
            AzResultU8VecFileError read_result = AzFilePath_readBytes(&pack_path);
            if (read_result.Ok.tag == AzResultU8VecFileError_Tag_Ok) {
                AzU8Vec data = read_result.Ok.payload;
                AzString content = AzString_copyFromBytes(data.ptr, 0, data.len);
                // Note: addResource takes ownership of locale and source strings
                bool ok = AzFluentLocalizerHandle_addResource(&localizer, az_str(lang_packs[i].locale), content);
                printf(" %s\n", ok ? "OK" : "FAILED");
                // Don't delete content - ownership transferred to addResource
                AzU8Vec_delete(&data);
            } else {
                printf(" READ FAILED\n");
            }
        } else {
            // "Download" (simulate) and cache
            printf("  %s: Downloading and caching...", lang_packs[i].locale);
            
            // In real app: AzHttpClient_get(url) to download
            // Here we just use the embedded content
            AzU8Vec content_bytes = AzU8Vec_copyFromBytes(
                (const uint8_t*)lang_packs[i].content, 
                0, 
                strlen(lang_packs[i].content)
            );
            
            // Write to cache
            AzResultVoidFileError write_result = AzFilePath_writeBytes(&pack_path, content_bytes);
            (void)write_result;
            
            // Load into localizer
            bool ok = AzFluentLocalizerHandle_addResource(
                &localizer, 
                az_str(lang_packs[i].locale), 
                az_str(lang_packs[i].content)
            );
            printf(" %s\n", ok ? "OK" : "FAILED");
        }
        
        AzFilePath_delete(&pack_path);
    }
    
    // Show loaded languages
    printf("\nLoaded languages:\n");
    AzStringVec locales = AzFluentLocalizerHandle_getLoadedLocales(&localizer);
    for (size_t i = 0; i < locales.len; i++) {
        AzString* locale = &((AzString*)locales.ptr)[i];
        WITH_CSTR(*locale, loc, {
            printf("  - %s\n", loc);
        });
    }
    AzStringVec_delete(&locales);
    
    // Demonstrate translation in all languages
    printf("\nTranslation demo (app-name):\n");
    AzFmtArgVec empty = { .ptr = NULL, .len = 0, .cap = 0, .destructor = AzFmtArgVecDestructor_noDestructor() };
    
    for (size_t i = 0; i < num_packs; i++) {
        AzString result = AzFluentLocalizerHandle_translate(
            &localizer, 
            az_str(lang_packs[i].locale), 
            az_str("app-name"), 
            empty
        );
        WITH_CSTR(result, text, {
            printf("  %s: %s\n", lang_packs[i].locale, text);
        });
        AzString_delete(&result);
    }
    
    // Cleanup
    AzFluentLocalizerHandle_delete(&localizer);
    AzFilePath_delete(&cache_dir);
    AzFilePath_delete(&temp);
    
    printf("\nNote: Language packs are now cached. Run again to see cached loading.\n");
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
    demo_language_packs();
    demo_syntax_check_bytes();
    
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    
    return 0;
}
