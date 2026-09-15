#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int32_t counter;
    double temperature;
    bool is_active;
} AppState;

void AppState_destructor(void* ptr) {
    (void)ptr;
}

AzJson AppState_toJson(AzRefAny refany);
AzResultRefAnyString AppState_fromJson(AzJson json);

AZ_REFLECT_JSON(AppState, AppState_destructor, AppState_toJson, AppState_fromJson)

AzJson AppState_toJson(AzRefAny refany) {
    AppStateRef ref = AppStateRef_create(&refany);
    if (!AppState_downcastRef(&refany, &ref)) {
        printf("[ERROR] Failed to downcast RefAny to AppState\n");
        return AzJson_null();
    }
    
    AzJsonKeyValue counter_kv = AzJsonKeyValue_create(
        AZ_STR("counter"), 
        AzJson_float((double)ref.ptr->counter)
    );
    
    AzJsonKeyValue temp_kv = AzJsonKeyValue_create(
        AZ_STR("temperature"), 
        AzJson_float(ref.ptr->temperature)
    );
    
    AzJsonKeyValue active_kv = AzJsonKeyValue_create(
        AZ_STR("is_active"), 
        AzJson_bool(ref.ptr->is_active)
    );
    
    AppStateRef_delete(&ref);
    
    AzJsonKeyValue entries_arr[3] = { counter_kv, temp_kv, active_kv };
    AzJsonKeyValueVec entries = AzJsonKeyValueVec_copyFromArray(entries_arr, 3);
    return AzJson_object(entries);
}

AzResultRefAnyString AppState_fromJson(AzJson json) {
    
    if (!AzJson_isObject(&json)) {
        return AzResultRefAnyString_err(AZ_STR("Expected JSON object"));
    }
    
    AzOptionJson counter_opt = AzJson_getKey(&json, AZ_STR("counter"));
    AzOptionJson temp_opt = AzJson_getKey(&json, AZ_STR("temperature"));
    AzOptionJson active_opt = AzJson_getKey(&json, AZ_STR("is_active"));
    
    if (AzOptionJson_isNone(&counter_opt)) {
        return AzResultRefAnyString_err(AZ_STR("Missing field: counter"));
    }
    if (AzOptionJson_isNone(&temp_opt)) {
        return AzResultRefAnyString_err(AZ_STR("Missing field: temperature"));
    }
    if (AzOptionJson_isNone(&active_opt)) {
        return AzResultRefAnyString_err(AZ_STR("Missing field: is_active"));
    }
    
    AzJson counter_json = counter_opt.Some.payload;
    AzJson temp_json = temp_opt.Some.payload;
    AzJson active_json = active_opt.Some.payload;
    
    if (!AzJson_isFloat(&counter_json)) {
        return AzResultRefAnyString_err(AZ_STR("counter must be a number"));
    }
    if (!AzJson_isFloat(&temp_json)) {
        return AzResultRefAnyString_err(AZ_STR("temperature must be a number"));
    }
    if (!AzJson_isBool(&active_json)) {
        return AzResultRefAnyString_err(AZ_STR("is_active must be a boolean"));
    }
    
    AppState state = {
        .counter = (int32_t)AzJson_asFloat(&counter_json).Some.payload,
        .temperature = AzJson_asFloat(&temp_json).Some.payload,
        .is_active = AzJson_asBool(&active_json).Some.payload
    };
    
    AzRefAny refany = AppState_upcast(state);
    return AzResultRefAnyString_ok(refany);
}

int main() {
    printf("=== RefAny JSON Serialization Example ===\n\n");
    
    printf("1. Creating AppState with initial values...\n");
    AppState initial_state = {
        .counter = 42,
        .temperature = 23.5,
        .is_active = true
    };
    
    printf("   counter: %d\n", initial_state.counter);
    printf("   temperature: %.2f\n", initial_state.temperature);
    printf("   is_active: %s\n\n", initial_state.is_active ? "true" : "false");
    
    AzRefAny refany = AppState_upcast(initial_state);

    printf("2. Checking JSON support...\n");
    bool can_serialize = AzRefAny_canSerialize(&refany);
    bool can_deserialize = AzRefAny_canDeserialize(&refany);
    printf("   can_serialize: %s\n", can_serialize ? "true" : "false");
    printf("   can_deserialize: %s\n\n", can_deserialize ? "true" : "false");
    
    printf("3. Serializing to JSON...\n");
    AzOptionJson json_opt = AzRefAny_serializeToJson(&refany);
    
    if (AzOptionJson_isNone(&json_opt)) {
        printf("   [ERROR] Serialization failed!\n");
        return 1;
    }
    
    AzJson json = json_opt.Some.payload;
    AzString json_str = AzJson_toStringPretty(&json);
    printf("   Result:\n%s\n\n", (const char*)json_str.vec.ptr);
    
    printf("4. Deserializing from JSON...\n");
    
    size_t deserialize_fn = AzRefAny_getDeserializeFn(&refany);
    printf("   deserialize_fn: 0x%lx\n", (unsigned long)deserialize_fn);
    
    AzString modified_str = AZ_STR("{\"counter\": 100, \"temperature\": 98.6, \"is_active\": false}");
    AzResultJsonJsonParseError parse_result = AzJson_parse(modified_str);
    
    if (!AzResultJsonJsonParseError_isOk(&parse_result)) {
        printf("   [ERROR] Failed to parse modified JSON\n");
        return 1;
    }
    
    AzJson modified = parse_result.Ok.payload;
    
    AzResultRefAnyString deser_result = AzJson_deserializeToRefany(modified, deserialize_fn);
    
    if (AzResultRefAnyString_isErr(&deser_result)) {
        printf("   [ERROR] Deserialization failed: %s\n", (const char*)deser_result.Err.payload.vec.ptr);
        return 1;
    }
    
    AzRefAny new_refany = deser_result.Ok.payload;
    printf("   Deserialization successful!\n\n");
    
    printf("5. Verifying deserialized data...\n");
    AppStateRef new_ref = AppStateRef_create(&new_refany);
    if (AppState_downcastRef(&new_refany, &new_ref)) {
        printf("   counter: %d (expected: 100)\n", new_ref.ptr->counter);
        printf("   temperature: %.2f (expected: 98.60)\n", new_ref.ptr->temperature);
        printf("   is_active: %s (expected: false)\n\n", new_ref.ptr->is_active ? "true" : "false");
        AppStateRef_delete(&new_ref);
    } else {
        printf("   [ERROR] Failed to downcast deserialized RefAny\n");
    }
    
    printf("6. Round-trip test: serializing deserialized value...\n");
    AzOptionJson roundtrip_opt = AzRefAny_serializeToJson(&new_refany);
    if (AzOptionJson_isSome(&roundtrip_opt)) {
        AzJson roundtrip = roundtrip_opt.Some.payload;
        AzString roundtrip_str = AzJson_toStringPretty(&roundtrip);
        printf("   Result:\n%s\n\n", (const char*)roundtrip_str.vec.ptr);
        AzString_delete(&roundtrip_str);
        AzJson_delete(&roundtrip);
    }
    
    printf("7. Cleanup...\n");
    AzString_delete(&json_str);
    AzJson_delete(&json);
    AzJson_delete(&modified);
    AzRefAny_delete(&refany);
    AzRefAny_delete(&new_refany);
    
    printf("\n=== Example completed successfully! ===\n");
    return 0;
}
