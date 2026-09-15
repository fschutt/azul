#include "azul.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

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

void demo_locale(const char* locale_name, const char* locale_code) {
    printf("\n============================================================\n");
    printf("Locale: %s (%s)\n", locale_name, locale_code);
    printf("============================================================\n");
    
    AzString locale = az_str(locale_code);
    
    AzIcuLocalizerHandle cache = AzIcuLocalizerHandle_create(AzString_clone(&locale));
    
    printf("\n--- Number Formatting ---\n");
    int64_t number = 1234567;
    AzString formatted = AzIcuLocalizerHandle_formatInteger(&cache, AzString_clone(&locale), number);
    printf("Raw:       %lld\n", (long long)number);
    WITH_CSTR(formatted, s, { printf("Formatted: %s\n", s); });
    AzString_delete(&formatted);
    
    printf("\n--- Plural Rules ---\n");
    int64_t counts[] = {0, 1, 2, 5, 21};
    for (int i = 0; i < 5; i++) {
        int64_t count = counts[i];
        AzPluralCategory category = AzIcuLocalizerHandle_getPluralCategory(&cache, AzString_clone(&locale), count);
        
        AzString message = AzIcuLocalizerHandle_pluralize(&cache, AzString_clone(&locale), count,
            az_str("{} items"),
            az_str("{} item"),
            az_str("{} items"),
            az_str("{} items"),
            az_str("{} items"),
            az_str("{} items"));
        
        const char* category_str;
        switch (category) {
            case AzPluralCategory_Zero: category_str = "Zero"; break;
            case AzPluralCategory_One: category_str = "One"; break;
            case AzPluralCategory_Two: category_str = "Two"; break;
            case AzPluralCategory_Few: category_str = "Few"; break;
            case AzPluralCategory_Many: category_str = "Many"; break;
            default: category_str = "Other"; break;
        }
        
        WITH_CSTR(message, msg, { printf("count=%2lld: '%s' (category: %s)\n", (long long)count, msg, category_str); });
        
        AzString_delete(&message);
    }
    
    printf("\n--- Date/Time Formatting ---\n");
    AzIcuDate date = { .year = 2025, .month = 1, .day = 15 };
    AzIcuTime time = { .hour = 16, .minute = 30, .second = 45 };
    AzIcuDateTime datetime = { .date = date, .time = time };
    
    AzIcuResult date_short = AzIcuLocalizerHandle_formatDate(&cache, AzString_clone(&locale), date, AzFormatLength_Short);
    if (date_short.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(date_short.Ok.payload, s, { printf("Date (Short):  %s\n", s); });
    }
    AzIcuResult_delete(&date_short);
    
    AzIcuResult date_medium = AzIcuLocalizerHandle_formatDate(&cache, AzString_clone(&locale), date, AzFormatLength_Medium);
    if (date_medium.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(date_medium.Ok.payload, s, { printf("Date (Medium): %s\n", s); });
    }
    AzIcuResult_delete(&date_medium);
    
    AzIcuResult date_long = AzIcuLocalizerHandle_formatDate(&cache, AzString_clone(&locale), date, AzFormatLength_Long);
    if (date_long.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(date_long.Ok.payload, s, { printf("Date (Long):   %s\n", s); });
    }
    AzIcuResult_delete(&date_long);
    
    AzIcuResult time_short = AzIcuLocalizerHandle_formatTime(&cache, AzString_clone(&locale), time, false);
    if (time_short.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(time_short.Ok.payload, s, { printf("Time (short):  %s\n", s); });
    }
    AzIcuResult_delete(&time_short);
    
    AzIcuResult time_long = AzIcuLocalizerHandle_formatTime(&cache, AzString_clone(&locale), time, true);
    if (time_long.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(time_long.Ok.payload, s, { printf("Time (long):   %s\n", s); });
    }
    AzIcuResult_delete(&time_long);
    
    AzIcuResult dt_result = AzIcuLocalizerHandle_formatDatetime(&cache, AzString_clone(&locale), datetime, AzFormatLength_Long);
    if (dt_result.Ok.tag == AzIcuResult_Tag_Ok) {
        WITH_CSTR(dt_result.Ok.payload, s, { printf("DateTime:      %s\n", s); });
    }
    AzIcuResult_delete(&dt_result);
    
    printf("\n--- String Comparison ---\n");
    AzString str_a = az_str("Ägypten");
    AzString str_b = az_str("Bahamas");
    int32_t cmp = AzIcuLocalizerHandle_compareStrings(&cache, AzString_clone(&locale), str_a, str_b);
    const char* cmp_str = cmp < 0 ? "<" : (cmp > 0 ? ">" : "==");
    printf("'Ägypten' %s 'Bahamas' (result: %d)\n", cmp_str, cmp);
    
    AzString_delete(&locale);
    AzIcuLocalizerHandle_delete(&cache);
}

void demo_multi_locale() {
    printf("\n============================================================\n");
    printf("Multi-Locale Demo (Single Cache)\n");
    printf("============================================================\n");
    
    AzString default_locale = az_str("en-US");
    AzIcuLocalizerHandle cache = AzIcuLocalizerHandle_create(default_locale);
    
    int64_t number = 1234567;
    printf("\nFormatting %lld in different locales:\n", (long long)number);
    
    const char* locales[] = {"en-US", "de-DE", "fr-FR", "ja-JP"};
    const char* names[] = {"English (US)", "German", "French", "Japanese"};
    
    for (int i = 0; i < 4; i++) {
        AzString formatted = AzIcuLocalizerHandle_formatInteger(&cache, az_str(locales[i]), number);
        WITH_CSTR(formatted, s, { printf("  %s: %s\n", names[i], s); });
        AzString_delete(&formatted);
    }
    
    printf("\nPlural rules for count=2 in different locales:\n");
    for (int i = 0; i < 4; i++) {
        AzPluralCategory category = AzIcuLocalizerHandle_getPluralCategory(&cache, az_str(locales[i]), 2);
        
        const char* category_str;
        switch (category) {
            case AzPluralCategory_Zero: category_str = "Zero"; break;
            case AzPluralCategory_One: category_str = "One"; break;
            case AzPluralCategory_Two: category_str = "Two"; break;
            case AzPluralCategory_Few: category_str = "Few"; break;
            case AzPluralCategory_Many: category_str = "Many"; break;
            default: category_str = "Other"; break;
        }
        
        printf("  %s: %s\n", names[i], category_str);
    }
    
    AzIcuLocalizerHandle_delete(&cache);
}

int main() {
    printf("##########################################################\n");
    printf("#       ICU4X Internationalization Demo for Azul         #\n");
    printf("##########################################################\n");
    printf("\n");
    printf("This demo shows how ICU4X provides locale-aware formatting\n");
    printf("for numbers, dates, plurals, lists, and string sorting.\n");
    printf("\n");
    printf("All functions take a locale parameter, allowing\n");
    printf("    dynamic language switching per-call!\n");
    
    demo_locale("English (US)", "en-US");
    demo_locale("German", "de-DE");
    demo_locale("French", "fr-FR");
    demo_locale("Spanish", "es-ES");
    demo_locale("Japanese", "ja-JP");
    
    demo_multi_locale();
    
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    
    return 0;
}
