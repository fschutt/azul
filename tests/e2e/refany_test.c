/**
 * RefAny Reference Counting Test
 * 
 * Tests that RefAny properly handles:
 * 1. Clone increments num_copies
 * 2. Drop decrements num_copies
 * 3. RefCount clone (for Ref/RefMut) keeps data alive
 * 4. Memory is freed only when last reference is dropped
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct {
    int value;
    int id;
} TestData;

static int destructor_call_count = 0;
static int last_destroyed_id = -1;

void TestData_destructor(void* data) {
    TestData* td = (TestData*)data;
    destructor_call_count++;
    last_destroyed_id = td->id;
    fprintf(stderr, "[DESTRUCTOR] TestData id=%d destroyed (total destructor calls: %d)\n", 
            td->id, destructor_call_count);
    fflush(stderr);
}

AZ_REFLECT(TestData, TestData_destructor)

int test_basic_clone_and_drop() {
    fprintf(stderr, "\n=== Test 1: Basic Clone and Drop ===\n");
    int start_count = destructor_call_count;
    
    TestData data1 = { .value = 42, .id = 1 };
    fprintf(stderr, "Creating RefAny with id=1\n");
    AzRefAny ref1 = TestData_upcast(data1);
    fprintf(stderr, "ref1 created\n");
    
    fprintf(stderr, "Cloning ref1 -> ref2\n");
    AzRefAny ref2 = AzRefAny_clone(&ref1);
    fprintf(stderr, "ref2 created (clone of ref1)\n");
    
    fprintf(stderr, "Deleting ref1...\n");
    AzRefAny_delete(&ref1);
    int after_first_delete = destructor_call_count - start_count;
    fprintf(stderr, "ref1 deleted, new destructor calls = %d (expected: 0)\n", after_first_delete);
    
    if (after_first_delete != 0) {
        fprintf(stderr, "[FAIL] Destructor called too early!\n");
        return 1;
    }
    fprintf(stderr, "[OK] Destructor not called yet (ref2 still exists)\n");
    
    fprintf(stderr, "Deleting ref2 (last reference)...\n");
    AzRefAny_delete(&ref2);
    int after_second_delete = destructor_call_count - start_count;
    fprintf(stderr, "ref2 deleted, new destructor calls = %d (expected: 1)\n", after_second_delete);
    
    if (after_second_delete != 1) {
        fprintf(stderr, "[FAIL] Destructor should have been called exactly once!\n");
        return 1;
    }
    fprintf(stderr, "[PASS] Test 1 passed!\n");
    return 0;
}

int test_refcount_clone_keeps_alive() {
    fprintf(stderr, "\n=== Test 2: RefCount Clone Keeps Data Alive ===\n");
    int start_count = destructor_call_count;
    
    TestData data2 = { .value = 100, .id = 2 };
    fprintf(stderr, "Creating RefAny with id=2\n");
    AzRefAny ref = TestData_upcast(data2);
    
    // Simulate what happens in layout callback:
    // 1. Create a Ref by cloning RefCount and getting data pointer
    fprintf(stderr, "Creating TestDataRef (simulating downcastRef)...\n");
    TestDataRef dataRef = TestDataRef_create(&ref);
    
    fprintf(stderr, "Attempting downcast...\n");
    bool success = TestData_downcastRef(&ref, &dataRef);
    
    if (!success) {
        fprintf(stderr, "[FAIL] downcastRef failed!\n");
        AzRefAny_delete(&ref);
        return 1;
    }
    
    fprintf(stderr, "downcastRef succeeded, dataRef.ptr->value = %d, id = %d\n", 
            dataRef.ptr->value, dataRef.ptr->id);
    
    // Now delete the original RefAny - data should still be accessible via dataRef
    fprintf(stderr, "Deleting original RefAny while Ref is still held...\n");
    AzRefAny_delete(&ref);
    int after_refany_delete = destructor_call_count - start_count;
    fprintf(stderr, "Original RefAny deleted, new destructor calls = %d (expected: 0)\n", after_refany_delete);
    
    if (after_refany_delete != 0) {
        fprintf(stderr, "[FAIL] Destructor called while Ref still exists!\n");
        TestDataRef_delete(&dataRef);
        return 1;
    }
    fprintf(stderr, "[OK] Data still alive, value = %d\n", dataRef.ptr->value);
    
    // Release the Ref - now destructor should be called
    fprintf(stderr, "Releasing TestDataRef...\n");
    TestDataRef_delete(&dataRef);
    int after_ref_delete = destructor_call_count - start_count;
    fprintf(stderr, "TestDataRef released, new destructor calls = %d (expected: 1)\n", after_ref_delete);
    
    if (after_ref_delete != 1) {
        fprintf(stderr, "[FAIL] Destructor should have been called!\n");
        return 1;
    }
    fprintf(stderr, "[PASS] Test 2 passed!\n");
    return 0;
}

int test_multiple_refs() {
    fprintf(stderr, "\n=== Test 3: Multiple Refs from Same RefAny ===\n");
    int start_count = destructor_call_count;
    
    TestData data3 = { .value = 200, .id = 3 };
    fprintf(stderr, "Creating RefAny with id=3\n");
    AzRefAny ref = TestData_upcast(data3);
    
    // Create multiple Refs
    fprintf(stderr, "Creating 3 TestDataRefs...\n");
    TestDataRef ref1 = TestDataRef_create(&ref);
    TestDataRef ref2 = TestDataRef_create(&ref);
    TestDataRef ref3 = TestDataRef_create(&ref);
    
    TestData_downcastRef(&ref, &ref1);
    TestData_downcastRef(&ref, &ref2);
    TestData_downcastRef(&ref, &ref3);
    
    fprintf(stderr, "3 Refs created, deleting original RefAny...\n");
    AzRefAny_delete(&ref);
    int count1 = destructor_call_count - start_count;
    fprintf(stderr, "destructor calls so far = %d (expected: 0)\n", count1);
    
    fprintf(stderr, "Deleting ref1...\n");
    TestDataRef_delete(&ref1);
    int count2 = destructor_call_count - start_count;
    fprintf(stderr, "destructor calls so far = %d (expected: 0)\n", count2);
    
    fprintf(stderr, "Deleting ref2...\n");
    TestDataRef_delete(&ref2);
    int count3 = destructor_call_count - start_count;
    fprintf(stderr, "destructor calls so far = %d (expected: 0)\n", count3);
    
    fprintf(stderr, "Deleting ref3 (last reference)...\n");
    TestDataRef_delete(&ref3);
    int count4 = destructor_call_count - start_count;
    fprintf(stderr, "destructor calls so far = %d (expected: 1)\n", count4);
    
    if (count1 != 0 || count2 != 0 || count3 != 0) {
        fprintf(stderr, "[FAIL] Destructor called too early!\n");
        return 1;
    }
    if (count4 != 1) {
        fprintf(stderr, "[FAIL] Destructor count wrong!\n");
        return 1;
    }
    fprintf(stderr, "[PASS] Test 3 passed!\n");
    return 0;
}

int main() {
    fprintf(stderr, "===========================================\n");
    fprintf(stderr, "RefAny Reference Counting Tests\n");
    fprintf(stderr, "===========================================\n");
    
    int failures = 0;
    failures += test_basic_clone_and_drop();
    failures += test_refcount_clone_keeps_alive();
    failures += test_multiple_refs();
    
    fprintf(stderr, "\n===========================================\n");
    if (failures == 0) {
        fprintf(stderr, "All tests PASSED!\n");
    } else {
        fprintf(stderr, "%d test(s) FAILED!\n", failures);
    }
    fprintf(stderr, "Total destructor calls: %d (expected: 3)\n", destructor_call_count);
    fprintf(stderr, "===========================================\n");
    
    return (failures == 0 && destructor_call_count == 3) ? 0 : 1;
}
