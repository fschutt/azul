#include <azul.h>
#include <stdio.h>
#include <string.h>

// Test ABI for passing AzRefAny by value
extern void AzRefAny_debugPrint(AzRefAny data);

int main() {
    // Create a RefAny
    printf("Creating RefAny...\n");
    uint32_t value = 42;
    
    // Manual creation to avoid the upcast macro
    AzRefAny data;
    memset(&data, 0, sizeof(data));
    printf("sizeof(AzRefAny) = %zu\n", sizeof(AzRefAny));
    printf("&data = %p\n", &data);
    
    return 0;
}
