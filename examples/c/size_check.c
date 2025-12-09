#include <azul.h>
#include <stdio.h>

int main() {
    printf("C Size checks:\n");
    printf("  sizeof(AzRefAny) = %zu\n", sizeof(AzRefAny));
    printf("  sizeof(AzRefCount) = %zu\n", sizeof(AzRefCount));
    printf("  alignof(AzRefAny) = %zu\n", _Alignof(AzRefAny));
    printf("  alignof(AzRefCount) = %zu\n", _Alignof(AzRefCount));
    printf("  sizeof(void*) = %zu\n", sizeof(void*));
    printf("  sizeof(uint64_t) = %zu\n", sizeof(uint64_t));
    printf("  sizeof(bool) = %zu\n", sizeof(bool));
    
    // Check field offsets
    AzRefAny r;
    printf("\nField offsets in AzRefAny:\n");
    printf("  _internal_ptr: %zu\n", (size_t)((char*)&r._internal_ptr - (char*)&r));
    printf("  sharing_info: %zu\n", (size_t)((char*)&r.sharing_info - (char*)&r));
    printf("  instance_id: %zu\n", (size_t)((char*)&r.instance_id - (char*)&r));
    printf("  run_destructor: %zu\n", (size_t)((char*)&r.run_destructor - (char*)&r));
    
    printf("\nApp sizes:\n");
    printf("  sizeof(AzApp) = %zu\n", sizeof(AzApp));
    AzApp a;
    printf("  offsetof(AzApp, ptr) = %zu\n", (size_t)((char*)&a.ptr - (char*)&a));
    printf("  offsetof(AzApp, run_destructor) = %zu\n", (size_t)((char*)&a.run_destructor - (char*)&a));
    
    printf("\nWindowCreateOptions sizes:\n");
    printf("  sizeof(AzWindowCreateOptions) = %zu\n", sizeof(AzWindowCreateOptions));
    printf("  sizeof(AzFullWindowState) = %zu\n", sizeof(AzFullWindowState));
    printf("  sizeof(AzAppConfig) = %zu\n", sizeof(AzAppConfig));
    
    return 0;
}